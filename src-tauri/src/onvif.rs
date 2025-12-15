use crate::models::{DiscoveredDevice, Camera};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use uuid::Uuid;
use roxmltree::Document;
use local_ip_address::local_ip;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use sha1::{Sha1, Digest};
use base64::prelude::*;
use chrono::{Utc, Datelike, Timelike};

const ONVIF_PORT: u16 = 3702;
const PROBE_TIMEOUT_MS: u64 = 2000;
const CONCURRENCY_LIMIT: usize = 50;

// --- Discovery (Existing) ---

pub async fn discover_devices() -> Result<Vec<DiscoveredDevice>, String> {
    let local_ip = local_ip().map_err(|e| format!("Failed to get local IP: {}", e))?;
    let ipv4 = match local_ip {
        IpAddr::V4(ip) => ip,
        _ => return Err("IPv6 not supported for simple subnet scan yet".to_string()),
    };

    let octets = ipv4.octets();
    let subnet_base = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
    
    println!("[Discovery] Scanning subnet: {}.1-254", subnet_base);

    let mut target_ips = Vec::new();
    for i in 1..=254 {
        target_ips.push(format!("{}.{}", subnet_base, i));
    }

    let tasks = target_ips.into_iter().map(|ip| {
        let ip_addr = ip.clone();
        async move {
            probe_ip(&ip_addr).await
        }
    });

    let results = stream::iter(tasks)
        .buffer_unordered(CONCURRENCY_LIMIT)
        .collect::<Vec<_>>()
        .await;

    let mut devices = Vec::new();
    for res in results {
        if let Some(device) = res {
            if !devices.iter().any(|d: &DiscoveredDevice| d.address == device.address) {
                devices.push(device);
            }
        }
    }
    
    println!("[Discovery] Found {} devices", devices.len());
    Ok(devices)
}

async fn probe_ip(ip: &str) -> Option<DiscoveredDevice> {
    let target: SocketAddr = format!("{}:{}", ip, ONVIF_PORT).parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    
    let uuid = Uuid::new_v4();
    let probe_xml = format!(
        r###"<?xml version="1.0" encoding="UTF-8"?>
<Envelope xmlns="http://www.w3.org/2003/05/soap-envelope" xmlns:dn="http://www.onvif.org/ver10/network/wsdl">
    <Header>
        <wsa:MessageID xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing">urn:uuid:{}</wsa:MessageID>
        <wsa:To xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing">urn:schemas-xmlsoap-org:ws:2005:04:discovery</wsa:To>
        <wsa:Action xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</wsa:Action>
    </Header>
    <Body>
        <Probe xmlns="http://schemas.xmlsoap.org/ws/2005/04/discovery">
            <Types>dn:NetworkVideoTransmitter</Types>
            <Scopes />
        </Probe>
    </Body>
</Envelope>"###,
        uuid
    );

    if let Err(_) = socket.send_to(probe_xml.as_bytes(), target).await {
        return None;
    }

    let mut buf = [0u8; 4096];
    let res = tokio::time::timeout(Duration::from_millis(PROBE_TIMEOUT_MS), socket.recv_from(&mut buf)).await;

    match res {
        Ok(Ok((len, _src))) => {
            let data = &buf[..len];
            if let Ok(xml_str) = std::str::from_utf8(data) {
                return parse_probe_match(xml_str, ip.to_string());
            }
        }
        _ => {}
    }

    None
}

fn parse_probe_match(xml: &str, ip_addr: String) -> Option<DiscoveredDevice> {
    let doc = Document::parse(xml).ok()?;
    
    let body = doc.root_element().descendants().find(|n| n.has_tag_name("Body"))?;
    let probe_matches = body.descendants().find(|n| n.tag_name().name().ends_with("ProbeMatches"))?;
    let probe_match = probe_matches.descendants().find(|n| n.tag_name().name().ends_with("ProbeMatch"))?;
    
    let xaddrs_node = probe_match.descendants().find(|n| n.tag_name().name().ends_with("XAddrs"))?;
    let xaddrs_text = xaddrs_node.text().unwrap_or("");
    let xaddr = xaddrs_text.split_whitespace().next().map(|s| s.to_string());

    let scopes_node = probe_match.descendants().find(|n| n.tag_name().name().ends_with("Scopes"))?;
    let scopes_text = scopes_node.text().unwrap_or("");
    
    let mut name = "Unknown Camera".to_string();
    let mut manufacturer = "Unknown".to_string();
    let mut hardware = "".to_string();

    for scope in scopes_text.split_whitespace() {
        let decoded_scope = urlencoding::decode(scope).unwrap_or(std::borrow::Cow::Borrowed(scope));
        let scope_str = decoded_scope.as_ref();

        if scope_str.contains("/name/") {
            name = scope_str.split("/name/").last().unwrap_or("").to_string();
        } else if scope_str.contains("/hardware/") {
            hardware = scope_str.split("/hardware/").last().unwrap_or("").to_string();
        }
    }
    
    if manufacturer == "Unknown" && !hardware.is_empty() {
        manufacturer = hardware;
    }
    
    let mut port = 80;
    if let Some(ref addr) = xaddr {
        if let Ok(url) = url::Url::parse(addr) {
            if let Some(p) = url.port() {
                port = p as i32;
            }
        }
    }

    Some(DiscoveredDevice {
        address: ip_addr,
        port,
        hostname: "".to_string(),
        name,
        manufacturer,
        xaddr,
    })
}

// --- ONVIF Stream URI Retrieval ---

fn generate_security_header(user: &str, pass: &str) -> String {
    let nonce_raw: [u8; 16] = rand::random();
    let nonce = BASE64_STANDARD.encode(nonce_raw);
    let created = Utc::now().format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let mut hasher = Sha1::new();
    hasher.update(&nonce_raw);
    hasher.update(created.as_bytes());
    hasher.update(pass.as_bytes());
    let password_digest = BASE64_STANDARD.encode(hasher.finalize());

    format!(
        r###"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
      <wsse:UsernameToken wsu:Id="UsernameToken-1">
        <wsse:Username>{}</wsse:Username>
        <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{}</wsse:Password>
        <wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{}</wsse:Nonce>
        <wsu:Created>{}</wsu:Created>
      </wsse:UsernameToken>
    </wsse:Security>"###,
        user, password_digest, nonce, created
    )
}

pub async fn get_onvif_stream_url(camera: &Camera) -> Result<String, String> {
    let xaddr = camera.xaddr.clone().ok_or("No xAddr available for ONVIF camera")?;
    let user = camera.user.clone().unwrap_or_default();
    let pass = camera.pass.clone().unwrap_or_default();
    
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    // 1. GetProfiles to get a ProfileToken
    let profiles_body = r###"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"###;
    let profiles_envelope = build_soap_envelope(&user, &pass, profiles_body);

    let profiles_res = client.post(&xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/media/wsdl/GetProfiles\"")
        .body(profiles_envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to GetProfiles: {}", e))?;
    
    let profiles_xml = profiles_res.text().await.map_err(|e| e.to_string())?;
    let profile_token = parse_first_profile_token(&profiles_xml).ok_or("Failed to parse ProfileToken")?;
    
    // 2. GetStreamUri with the token
    let stream_body = format!(
        r###"<GetStreamUri xmlns="http://www.onvif.org/ver10/media/wsdl">
      <StreamSetup>
        <Stream xmlns="http://www.onvif.org/ver10/schema">RTP-Unicast</Stream>
        <Transport xmlns="http://www.onvif.org/ver10/schema">
          <Protocol>RTSP</Protocol>
        </Transport>
      </StreamSetup>
      <ProfileToken>{}</ProfileToken>
    </GetStreamUri>"###,
        profile_token
    );
    let stream_envelope = build_soap_envelope(&user, &pass, &stream_body);

    let stream_res = client.post(&xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/media/wsdl/GetStreamUri\"")
        .body(stream_envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to GetStreamUri: {}", e))?;

    let stream_xml = stream_res.text().await.map_err(|e| e.to_string())?;
    let rtsp_uri = parse_stream_uri(&stream_xml).ok_or("Failed to parse Stream URI")?;

    // Inject credentials into RTSP URL
    let final_url = if !user.is_empty() {
        // Check if URL already has auth? Usually ONVIF returns raw URL.
        // We assume standard rtsp://host...
        if let Some(idx) = rtsp_uri.find("://") {
            let (scheme, rest) = rtsp_uri.split_at(idx + 3);
             // encode password
             let encoded_pass = urlencoding::encode(&pass);
             format!("{}{}:{}@{}", scheme, user, encoded_pass, rest)
        } else {
            rtsp_uri
        }
    } else {
        rtsp_uri
    };

    println!("[ONVIF] Resolved Stream URL: {}", final_url);
    Ok(final_url)
}

// --- PTZ Functions ---

pub async fn get_ptz_service_url(camera: &Camera) -> Result<String, String> {
    let xaddr = camera.xaddr.clone().ok_or("No xAddr available")?;
    let user = camera.user.clone().unwrap_or_default();
    let pass = camera.pass.clone().unwrap_or_default();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    // GetCapabilities
    let body = r###"<GetCapabilities xmlns="http://www.onvif.org/ver10/device/wsdl">
        <Category>PTZ</Category>
    </GetCapabilities>"###;
    let envelope = build_soap_envelope(&user, &pass, body);

    let res = client.post(&xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/device/wsdl/GetCapabilities\"")
        .body(envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to GetCapabilities: {}", e))?;

    let xml = res.text().await.map_err(|e| e.to_string())?;
    
    // Parse PTZ XAddr
    let re = Regex::new(r"(?s)<[^:]*:PTZ>.*?<[^:]*:XAddr>(.*?)</[^:]*:XAddr>").map_err(|e| e.to_string())?;
    if let Some(caps) = re.captures(&xml) {
        return Ok(caps[1].trim().to_string());
    }

    Err("PTZ Service not found in capabilities".to_string())
}

async fn get_profile_token(client: &Client, xaddr: &str, user: &str, pass: &str) -> Result<String, String> {
     let profiles_body = r###"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"###;
    let profiles_envelope = build_soap_envelope(user, pass, profiles_body);

    let profiles_res = client.post(xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/media/wsdl/GetProfiles\"")
        .body(profiles_envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to GetProfiles: {}", e))?;
    
    let profiles_xml = profiles_res.text().await.map_err(|e| e.to_string())?;
    parse_first_profile_token(&profiles_xml).ok_or("Failed to parse ProfileToken".to_string())
}

pub async fn continuous_move(camera: &Camera, x: f32, y: f32, zoom: f32) -> Result<(), String> {
    let ptz_url = get_ptz_service_url(camera).await?;
    let media_xaddr = camera.xaddr.clone().ok_or("No XAddr")?; // Assume Media Service is at Device XAddr for simplicity (often true or routed)
    let user = camera.user.clone().unwrap_or_default();
    let pass = camera.pass.clone().unwrap_or_default();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let token = get_profile_token(&client, &media_xaddr, &user, &pass).await?;

    let body = format!(
        r###"<ContinuousMove xmlns="http://www.onvif.org/ver20/ptz/wsdl">
      <ProfileToken>{}</ProfileToken>
      <Velocity>
        <PanTilt x="{}" y="{}" space="http://www.onvif.org/ver10/tptz/PanTiltSpaces/VelocityGenericSpace" xmlns="http://www.onvif.org/ver10/schema"/>
        <Zoom x="{}" space="http://www.onvif.org/ver10/tptz/ZoomSpaces/VelocityGenericSpace" xmlns="http://www.onvif.org/ver10/schema"/>
      </Velocity>
    </ContinuousMove>"###,
        token, x, y, zoom
    );
    let envelope = build_soap_envelope(&user, &pass, &body);

    client.post(&ptz_url)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver20/ptz/wsdl/ContinuousMove\"")
        .body(envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to ContinuousMove: {}", e))?;

    Ok(())
}

pub async fn stop_move(camera: &Camera) -> Result<(), String> {
    let ptz_url = get_ptz_service_url(camera).await?;
    let media_xaddr = camera.xaddr.clone().ok_or("No XAddr")?;
    let user = camera.user.clone().unwrap_or_default();
    let pass = camera.pass.clone().unwrap_or_default();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let token = get_profile_token(&client, &media_xaddr, &user, &pass).await?;

    let body = format!(
        r###"<Stop xmlns="http://www.onvif.org/ver20/ptz/wsdl">
      <ProfileToken>{}</ProfileToken>
      <PanTilt>true</PanTilt>
      <Zoom>true</Zoom>
    </Stop>"###,
        token
    );
    let envelope = build_soap_envelope(&user, &pass, &body);

    client.post(&ptz_url)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver20/ptz/wsdl/Stop\"")
        .body(envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to Stop move: {}", e))?;

    Ok(())
}

fn build_soap_envelope(user: &str, pass: &str, body_content: &str) -> String {
    let security_header = if !user.is_empty() {
        generate_security_header(user, pass)
    } else {
        "".to_string()
    };

    format!(
        r###"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
  <s:Header>
    {}
  </s:Header>
  <s:Body xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
    {}
  </s:Body>
</s:Envelope>"###,
        security_header, body_content
    )
}

use regex::Regex;



// ... (previous imports)



// ... (discover_devices and helper functions remain same)



// ... (get_onvif_stream_url remains same)



// ... (build_soap_envelope remains same)



fn parse_first_profile_token(xml: &str) -> Option<String> {

    // Regex to find token="VALUE" inside a tag ending with Profiles

    let re = Regex::new(r#"(?s)<[^>]*:Profiles[^>]*\stoken="([^"]+)""#).ok()?;

    if let Some(caps) = re.captures(xml) {

        return Some(caps[1].to_string());

    }

    

    // Fallback: Try searching for just token="..." if previous failed, 

    // but we must be careful. 

    // Let's try a simpler pattern if the first complex one fails, 

    // assuming the structure <xxx:Profiles ... token="yyy" ...>

    None

}



fn parse_stream_uri(xml: &str) -> Option<String> {

    // Regex to find <*:Uri>VALUE</*:Uri>

    // Handles namespaces like <tt:Uri>...</tt:Uri> or <Uri>...</Uri>

    let re = Regex::new(r"(?s)<[^:]*:?Uri>(.*?)</[^:]*:?Uri>").ok()?;

    if let Some(caps) = re.captures(xml) {

        return Some(caps[1].trim().to_string());

    }

    None

}

// --- Time Synchronization Functions ---

#[derive(Debug)]
pub struct ONVIFDateTime {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

impl ONVIFDateTime {
    pub fn from_chrono(dt: &chrono::DateTime<Utc>) -> Self {
        ONVIFDateTime {
            year: dt.year(),
            month: dt.month() as i32,
            day: dt.day() as i32,
            hour: dt.hour() as i32,
            minute: dt.minute() as i32,
            second: dt.second() as i32,
        }
    }

    pub fn to_chrono(&self) -> Option<chrono::DateTime<Utc>> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(
            self.year,
            self.month as u32,
            self.day as u32,
            self.hour as u32,
            self.minute as u32,
            self.second as u32,
        ).single()
    }
}

pub async fn get_system_date_time(camera: &Camera) -> Result<ONVIFDateTime, String> {
    let xaddr = camera.xaddr.clone().ok_or("No xAddr available for ONVIF camera")?;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    // GetSystemDateAndTime does not require authentication in ONVIF spec
    let body = r###"<GetSystemDateAndTime xmlns="http://www.onvif.org/ver10/device/wsdl"/>"###;

    // Use empty credentials for GetSystemDateAndTime (public endpoint)
    let envelope = build_soap_envelope("", "", body);

    let res = client.post(&xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/device/wsdl/GetSystemDateAndTime\"")
        .body(envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to GetSystemDateAndTime: {}", e))?;

    let xml = res.text().await.map_err(|e| e.to_string())?;

    parse_system_date_time(&xml)
}

fn parse_system_date_time(xml: &str) -> Result<ONVIFDateTime, String> {
    // Parse UTC DateTime from response
    // Expected structure: <UTCDateTime><Date><Year>...</Year><Month>...</Month><Day>...</Day></Date><Time>...</Time></UTCDateTime>

    let year_re = Regex::new(r"<[^:]*:?Year>(\d+)</[^:]*:?Year>").map_err(|e| e.to_string())?;
    let month_re = Regex::new(r"<[^:]*:?Month>(\d+)</[^:]*:?Month>").map_err(|e| e.to_string())?;
    let day_re = Regex::new(r"<[^:]*:?Day>(\d+)</[^:]*:?Day>").map_err(|e| e.to_string())?;
    let hour_re = Regex::new(r"<[^:]*:?Hour>(\d+)</[^:]*:?Hour>").map_err(|e| e.to_string())?;
    let minute_re = Regex::new(r"<[^:]*:?Minute>(\d+)</[^:]*:?Minute>").map_err(|e| e.to_string())?;
    let second_re = Regex::new(r"<[^:]*:?Second>(\d+)</[^:]*:?Second>").map_err(|e| e.to_string())?;

    let year = year_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Year")?;

    let month = month_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Month")?;

    let day = day_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Day")?;

    let hour = hour_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Hour")?;

    let minute = minute_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Minute")?;

    let second = second_re.captures(xml)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .ok_or("Failed to parse Second")?;

    Ok(ONVIFDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    })
}

pub async fn set_system_date_time(camera: &Camera, datetime: &ONVIFDateTime) -> Result<(), String> {
    let xaddr = camera.xaddr.clone().ok_or("No xAddr available for ONVIF camera")?;
    let user = camera.user.clone().unwrap_or_default();
    let pass = camera.pass.clone().unwrap_or_default();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let body = format!(
        r###"<SetSystemDateAndTime xmlns="http://www.onvif.org/ver10/device/wsdl">
      <DateTimeType>Manual</DateTimeType>
      <DaylightSavings>false</DaylightSavings>
      <TimeZone>
        <TZ xmlns="http://www.onvif.org/ver10/schema">UTC</TZ>
      </TimeZone>
      <UTCDateTime>
        <Date xmlns="http://www.onvif.org/ver10/schema">
          <Year>{}</Year>
          <Month>{}</Month>
          <Day>{}</Day>
        </Date>
        <Time xmlns="http://www.onvif.org/ver10/schema">
          <Hour>{}</Hour>
          <Minute>{}</Minute>
          <Second>{}</Second>
        </Time>
      </UTCDateTime>
    </SetSystemDateAndTime>"###,
        datetime.year, datetime.month, datetime.day,
        datetime.hour, datetime.minute, datetime.second
    );

    let envelope = build_soap_envelope(&user, &pass, &body);

    let res = client.post(&xaddr)
        .header("Content-Type", "application/soap+xml; charset=utf-8; action=\"http://www.onvif.org/ver10/device/wsdl/SetSystemDateAndTime\"")
        .body(envelope)
        .send()
        .await
        .map_err(|e| format!("Failed to SetSystemDateAndTime: {}", e))?;

    let status = res.status();
    let response_text = res.text().await.map_err(|e| e.to_string())?;

    println!("[ONVIF] SetSystemDateAndTime response status: {}", status);
    println!("[ONVIF] SetSystemDateAndTime response body: {}", response_text);

    if !status.is_success() {
        return Err(format!("SetSystemDateAndTime failed with status {}: {}", status, response_text));
    }

    // Check for SOAP fault
    if response_text.contains("Fault") || response_text.contains("fault") {
        return Err(format!("SOAP Fault returned: {}", response_text));
    }

    println!("[ONVIF] SetSystemDateAndTime succeeded");
    Ok(())
}
