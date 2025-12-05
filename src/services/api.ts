import { invoke } from '@tauri-apps/api/core';

export interface Camera {
  id: number;
  name: string;
  type: 'onvif' | 'rtsp';
  host: string;
  port: number;
  xaddr?: string | null;  // ONVIF only
  stream_path?: string | null;  // RTSP only
}

export type NewCamera = {
  name: string;
  type: 'onvif' | 'rtsp';
  host: string;
  port: number;
  user?: string;
  pass?: string;
  xaddr?: string;
  stream_path?: string;
};

export const getCameras = async (): Promise<Camera[]> => {
  return await invoke('get_cameras');
};

export const addCamera = async (camera: NewCamera): Promise<Camera> => {
  return await invoke('add_camera', { camera });
};

export const deleteCamera = async (id: number): Promise<void> => {
  await invoke('delete_camera', { id });
};

export interface DiscoveredDevice {
  address: string;
  port: number;
  hostname: string;
  name: string;
  manufacturer: string;
  xaddr: string | null;
}

export const discoverCameras = async (): Promise<DiscoveredDevice[]> => {
  // Subnet scan is handled in Rust, which might take time.
  return await invoke('discover_cameras');
};

export const startStream = async (id: number): Promise<{ streamUrl: string }> => {
  return await invoke('start_stream', { id });
};

export const stopStream = async (id: number): Promise<{ success: boolean }> => {
  return await invoke('stop_stream', { id });
};

export const startRecording = async (id: number): Promise<{ success: boolean }> => {
  return await invoke('start_recording', { id });
};

export const stopRecording = async (id: number): Promise<{ success: boolean }> => {
  return await invoke('stop_recording', { id });
};

export interface Recording {
  id: number;
  filename: string;
  start_time: string;
  end_time: string;
  camera_name: string;
  thumbnail: string | null;
}

export const getRecordings = async (): Promise<Recording[]> => {
  return await invoke('get_recordings');
};

export const deleteRecording = async (id: number): Promise<void> => {
  await invoke('delete_recording', { id });
};

export interface CameraTimeInfo {
  cameraTime: any;
  serverTime: string;
}

export const getCameraTime = async (id: number): Promise<CameraTimeInfo> => {
  return await invoke('get_camera_time', { id });
};

export interface TimeSyncResult {
  success: boolean;
  beforeTime: any;
  serverTime: string;
  message: string;
  error?: string;
}

export const syncCameraTime = async (id: number): Promise<TimeSyncResult> => {
  return await invoke('sync_camera_time', { id });
};

export interface PTZCapabilities {
  supported: boolean;
  capabilities: {
    hasPanTilt: boolean;
    hasZoom: boolean;
  } | null;
}

export interface PTZMovement {
  x?: number;
  y?: number;
  zoom?: number;
  timeout?: number;
}

export interface PTZResult {
  success: boolean;
  message: string;
}

export const checkPTZCapabilities = async (id: number): Promise<PTZCapabilities> => {
  return await invoke('check_ptz_capabilities', { id });
};

export const movePTZ = async (id: number, movement: PTZMovement): Promise<PTZResult> => {
  return await invoke('move_ptz', { id, movement });
};

export const stopPTZ = async (id: number): Promise<PTZResult> => {
  return await invoke('stop_ptz', { id });
};

export interface CameraCapabilities {
  streaming: boolean;
  recording: boolean;
  thumbnails: boolean;
  ptz: boolean;
  discovery: boolean;
  timeSync: boolean;
  remoteAccess: boolean;
}

export const getCameraCapabilities = async (id: number): Promise<CameraCapabilities> => {
  // This might be computed on frontend or backend. Let's assume backend for consistency.
  return await invoke('get_camera_capabilities', { id });
};