use crate::models::EncoderSettings;
use crate::gpu_detector::{GpuCapabilities, test_encoder};

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub codec: String,
    pub args: Vec<String>,
    pub is_gpu: bool,
}

pub struct EncoderSelector {
    pub capabilities: GpuCapabilities,
    pub settings: EncoderSettings,
}

impl EncoderSelector {
    pub fn new(capabilities: GpuCapabilities, settings: EncoderSettings) -> Self {
        EncoderSelector {
            capabilities,
            settings,
        }
    }

    pub async fn select_encoder_for_streaming(&self, fps: Option<i32>) -> EncoderConfig {
        match self.settings.encoderMode.as_str() {
            "Auto" => {
                // Try GPU first, fallback to CPU
                if let Some(gpu_enc) = &self.settings.gpuEncoder {
                    if self.capabilities.availableEncoders.contains(gpu_enc) {
                        println!("[Encoder] Auto mode: trying GPU encoder {}", gpu_enc);
                        if test_encoder(gpu_enc).await {
                            return self.build_gpu_config_streaming(gpu_enc, fps);
                        }
                        println!("[Encoder] GPU encoder test failed, falling back to CPU");
                    }
                }
                // Fallback to CPU
                println!("[Encoder] Using CPU encoder (fallback)");
                self.build_cpu_config_streaming(fps)
            }
            "GpuOnly" => {
                // GPU only, no fallback
                let gpu_enc = self.settings.gpuEncoder.as_ref()
                    .expect("GPU encoder must be set for GpuOnly mode");
                println!("[Encoder] GpuOnly mode: using {}", gpu_enc);
                self.build_gpu_config_streaming(gpu_enc, fps)
            }
            "CpuOnly" => {
                // CPU only
                println!("[Encoder] CpuOnly mode: using {}", self.settings.cpuEncoder);
                self.build_cpu_config_streaming(fps)
            }
            _ => {
                println!("[Encoder] Unknown encoder mode, defaulting to CPU");
                self.build_cpu_config_streaming(fps)
            }
        }
    }

    pub async fn select_encoder_for_recording(&self) -> EncoderConfig {
        // Recording can use slightly different settings (higher quality)
        match self.settings.encoderMode.as_str() {
            "Auto" => {
                if let Some(gpu_enc) = &self.settings.gpuEncoder {
                    if self.capabilities.availableEncoders.contains(gpu_enc) {
                        if test_encoder(gpu_enc).await {
                            return self.build_gpu_config_recording(gpu_enc);
                        }
                    }
                }
                self.build_cpu_config_recording()
            }
            "GpuOnly" => {
                let gpu_enc = self.settings.gpuEncoder.as_ref()
                    .expect("GPU encoder must be set for GpuOnly mode");
                self.build_gpu_config_recording(gpu_enc)
            }
            "CpuOnly" => {
                self.build_cpu_config_recording()
            }
            _ => self.build_cpu_config_recording(),
        }
    }

    fn build_gpu_config_streaming(&self, encoder: &str, fps: Option<i32>) -> EncoderConfig {
        let mut args = Vec::new();

        // Calculate keyframe interval: fps * 2 for 2-second segments
        // Default to 60 if FPS not provided (for ONVIF cameras)
        let keyframe_interval = fps.map(|f| f * 2).unwrap_or(60).to_string();
        println!("[Encoder] Using keyframe interval: {} (FPS: {:?})", keyframe_interval, fps);

        match encoder {
            "h264_nvenc" | "hevc_nvenc" => {
                // NVIDIA NVENC settings for low-latency streaming
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-preset".to_string(), "p1".to_string(),     // p1 = fastest
                    "-tune".to_string(), "ll".to_string(),       // ultra-low latency
                    "-zerolatency".to_string(), "1".to_string(),
                    "-rc".to_string(), "cbr".to_string(),        // constant bitrate
                    "-b:v".to_string(), "4M".to_string(),
                    "-maxrate".to_string(), "4M".to_string(),
                    "-bufsize".to_string(), "2M".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                    "-bf".to_string(), "0".to_string(),          // no B-frames
                ]);
            }
            "h264_qsv" | "hevc_qsv" => {
                // Intel QSV settings - requires hardware initialization
                args.extend_from_slice(&[
                    "-init_hw_device".to_string(), "qsv=hw".to_string(),
                    "-filter_hw_device".to_string(), "hw".to_string(),
                    "-c:v".to_string(), encoder.to_string(),
                    "-preset".to_string(), "veryfast".to_string(),
                    "-global_quality".to_string(), self.settings.quality.to_string(),
                    "-look_ahead".to_string(), "0".to_string(),  // disable for low latency
                    "-b:v".to_string(), "4M".to_string(),
                    "-maxrate".to_string(), "4M".to_string(),
                    "-bufsize".to_string(), "2M".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-sc_threshold".to_string(), "0".to_string(),  // disable scene change detection
                ]);
            }
            "h264_amf" | "hevc_amf" => {
                // AMD AMF settings
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-quality".to_string(), "speed".to_string(),
                    "-rc".to_string(), "cbr".to_string(),
                    "-b:v".to_string(), "4M".to_string(),
                    "-maxrate".to_string(), "4M".to_string(),
                    "-bufsize".to_string(), "2M".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            "h264_vaapi" | "hevc_vaapi" => {
                // VA-API settings (Linux) - requires hardware initialization
                args.extend_from_slice(&[
                    "-init_hw_device".to_string(), "vaapi=va:/dev/dri/renderD128".to_string(),
                    "-filter_hw_device".to_string(), "va".to_string(),
                    "-c:v".to_string(), encoder.to_string(),
                    "-qp".to_string(), self.settings.quality.to_string(),
                    "-quality".to_string(), "1".to_string(),     // 1=speed, 4=quality
                    "-b:v".to_string(), "4M".to_string(),
                    "-maxrate".to_string(), "4M".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            "h264_videotoolbox" | "hevc_videotoolbox" => {
                // VideoToolbox settings (macOS)
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-b:v".to_string(), "4M".to_string(),
                    "-maxrate".to_string(), "4M".to_string(),
                    "-bufsize".to_string(), "2M".to_string(),
                    "-realtime".to_string(), "1".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            _ => {
                println!("[Encoder] Unknown GPU encoder {}, using defaults", encoder);
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-b:v".to_string(), "4M".to_string(),
                    "-g".to_string(), keyframe_interval.clone(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
        }

        EncoderConfig {
            codec: encoder.to_string(),
            args,
            is_gpu: true,
        }
    }

    fn build_cpu_config_streaming(&self, fps: Option<i32>) -> EncoderConfig {
        // Calculate keyframe interval: fps * 2 for 2-second segments
        // Default to 60 if FPS not provided (for ONVIF cameras)
        let keyframe_interval = fps.map(|f| f * 2).unwrap_or(60).to_string();
        println!("[Encoder] CPU using keyframe interval: {} (FPS: {:?})", keyframe_interval, fps);

        // Current CPU configuration (from stream.rs)
        let args = vec![
            "-c:v".to_string(), self.settings.cpuEncoder.clone(),
            "-preset".to_string(), self.settings.preset.clone(),
            "-tune".to_string(), "zerolatency".to_string(),
            "-g".to_string(), keyframe_interval.clone(),
            "-keyint_min".to_string(), keyframe_interval.clone(),
            "-sc_threshold".to_string(), "0".to_string(),
            "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),
        ];

        EncoderConfig {
            codec: self.settings.cpuEncoder.clone(),
            args,
            is_gpu: false,
        }
    }

    fn build_gpu_config_recording(&self, encoder: &str) -> EncoderConfig {
        let mut args = Vec::new();

        match encoder {
            "h264_nvenc" | "hevc_nvenc" => {
                // Higher quality for recording
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-preset".to_string(), "p4".to_string(),     // balanced preset
                    "-rc".to_string(), "vbr".to_string(),        // variable bitrate
                    "-cq".to_string(), self.settings.quality.to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-maxrate".to_string(), "10M".to_string(),
                    "-bufsize".to_string(), "8M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            "h264_qsv" | "hevc_qsv" => {
                // Intel QSV settings - requires hardware initialization
                args.extend_from_slice(&[
                    "-init_hw_device".to_string(), "qsv=hw".to_string(),
                    "-filter_hw_device".to_string(), "hw".to_string(),
                    "-c:v".to_string(), encoder.to_string(),
                    "-preset".to_string(), "medium".to_string(),
                    "-global_quality".to_string(), self.settings.quality.to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-maxrate".to_string(), "10M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-sc_threshold".to_string(), "0".to_string(),  // disable scene change detection
                ]);
            }
            "h264_amf" | "hevc_amf" => {
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-quality".to_string(), "balanced".to_string(),
                    "-rc".to_string(), "vbr_latency".to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-maxrate".to_string(), "10M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            "h264_vaapi" | "hevc_vaapi" => {
                // VA-API settings (Linux) - requires hardware initialization
                args.extend_from_slice(&[
                    "-init_hw_device".to_string(), "vaapi=va:/dev/dri/renderD128".to_string(),
                    "-filter_hw_device".to_string(), "va".to_string(),
                    "-c:v".to_string(), encoder.to_string(),
                    "-qp".to_string(), self.settings.quality.to_string(),
                    "-quality".to_string(), "2".to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-maxrate".to_string(), "10M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            "h264_videotoolbox" | "hevc_videotoolbox" => {
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-maxrate".to_string(), "10M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
            _ => {
                args.extend_from_slice(&[
                    "-c:v".to_string(), encoder.to_string(),
                    "-b:v".to_string(), "8M".to_string(),
                    "-g".to_string(), "120".to_string(),
                    "-force_key_frames".to_string(), "expr:gte(t,n_forced*2)".to_string(),  // force keyframe every 2 seconds
                ]);
            }
        }

        EncoderConfig {
            codec: encoder.to_string(),
            args,
            is_gpu: true,
        }
    }

    fn build_cpu_config_recording(&self) -> EncoderConfig {
        let args = vec![
            "-c:v".to_string(), self.settings.cpuEncoder.clone(),
            "-preset".to_string(), self.settings.preset.clone(),
        ];

        EncoderConfig {
            codec: self.settings.cpuEncoder.clone(),
            args,
            is_gpu: false,
        }
    }
}
