import { useState, useEffect, useRef, useCallback } from 'react';
import { AppBar, Toolbar, Typography, Container, CssBaseline, CircularProgress, Alert, Button, Modal, Paper, IconButton } from '@mui/material';
import SettingsIcon from '@mui/icons-material/Settings';
import CameraList from './components/CameraList';
import VideoPlayer from './components/VideoPlayer';
import RecordingList from './components/RecordingList';
import AddCameraModal from './components/AddCameraModal';
import DiscoverCamerasModal from './components/DiscoverCamerasModal';
import PTZControls from './components/PTZControls';
import EncoderSettings from './components/EncoderSettings';
import ScheduleRecording from './components/ScheduleRecording';
import { getCameras, startStream, stopStream, startRecording, stopRecording, checkPTZCapabilities } from './services/api';
import type { Camera } from './services/api';

// Style for the modal (keeping MUI sx for complex overlay centering if tailwind is tricky, but Tailwind is better)
// Tailwind: absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[80vw] bg-white border-2 border-black shadow-xl p-4
const modalClassName = "absolute top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 w-[80vw] bg-white border-2 border-black shadow-xl p-8 outline-none";

// Helper function to delay execution
const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

/**
 * Polls a URL with HEAD requests until it returns a 200 OK status.
 */
async function pollForStream(url: string, timeout = 30000, interval = 1000): Promise<void> {
  const startTime = Date.now();
  while (Date.now() - startTime < timeout) {
    try {
      const response = await fetch(url, { method: 'HEAD', cache: 'no-store' });
      if (response.ok) {
        console.log(`Stream manifest found at ${url}`);
        return;
      }
      console.log(`Polling for stream... status: ${response.status}`);
    } catch (error) {
      console.log('Polling request failed, retrying...', error);
    }
    await sleep(interval);
  }
  throw new Error(`Timed out after ${timeout / 1000}s waiting for stream to become available.`);
}

const SESSION_STORAGE_KEY = 'activeCameraIds';
const MAX_CAMERAS = 4;

// Type for active camera state
interface ActiveCameraState {
  camera: Camera;
  streamUrl: string | null;
  isLoadingStream: boolean;
  streamError: string | null;
  recordingStatus: 'idle' | 'recording';
  hasPTZ: boolean;
  checkingPTZ: boolean;
}

function App() {
  const [cameras, setCameras] = useState<Camera[]>([]);
  const [camerasLoading, setCamerasLoading] = useState<boolean>(true);
  const [camerasError, setCamerasError] = useState<string | null>(null);

  const [activeCameras, setActiveCameras] = useState<Map<number, ActiveCameraState>>(new Map());

  const [isPlaybackModalOpen, setIsPlaybackModalOpen] = useState(false);
  const [playingRecordingUrl, setPlayingRecordingUrl] = useState<string | null>(null);

  const [isAddCameraModalOpen, setIsAddCameraModalOpen] = useState(false);
  const [isDiscoverModalOpen, setIsDiscoverModalOpen] = useState(false);
  const [isEncoderSettingsOpen, setIsEncoderSettingsOpen] = useState(false);

  const [recordingListVersion, setRecordingListVersion] = useState(0);

  const stateRef = useRef({ activeCameras });
  useEffect(() => {
    stateRef.current = { activeCameras };
  });

  const fetchCameras = useCallback(async (cameraIdsToRestore?: number[]) => {
    try {
      setCamerasLoading(true);
      const camerasData = await getCameras();
      setCameras(camerasData);
      setCamerasError(null);

      if (cameraIdsToRestore && cameraIdsToRestore.length > 0) {
        cameraIdsToRestore.forEach(cameraId => {
          const cameraToRestore = camerasData.find(c => c.id === cameraId);
          if (cameraToRestore) {
            console.log(`Restoring stream for camera: ${cameraToRestore.name}`);
            setTimeout(() => handleSelectCamera(cameraToRestore), 0);
          }
        });
      }
    } catch (err) {
      setCamerasError('Failed to fetch cameras. Is the backend running?');
      console.error(err);
    } finally {
      setCamerasLoading(false);
    }
  }, []);

  useEffect(() => {
    const savedCameraIds = sessionStorage.getItem(SESSION_STORAGE_KEY);
    const idsToRestore = savedCameraIds ? JSON.parse(savedCameraIds) : undefined;
    fetchCameras(idsToRestore);
  }, [fetchCameras]);


  useEffect(() => {
    const handleCleanup = async (isUnloading = false) => {
      const { activeCameras: currentActiveCameras } = stateRef.current;
      
      const promises: Promise<any>[] = [];
      
      currentActiveCameras.forEach((cameraState, cameraId) => {
        if (cameraState.recordingStatus === 'recording') {
            promises.push(stopRecording(cameraId));
        }
        promises.push(stopStream(cameraId));
      });

      if (!isUnloading) {
          // If unmounting component, await cleanup. If unloading page, we can't await easily.
          // Tauri app close might be different.
          await Promise.allSettled(promises);
      }
    };

    const handleBeforeUnload = () => handleCleanup(true);
    window.addEventListener('beforeunload', handleBeforeUnload);

    return () => {
      window.removeEventListener('beforeunload', handleBeforeUnload);
      handleCleanup(false);
    };
  }, []);

  const handleSelectCamera = async (camera: Camera) => {
    const cameraId = camera.id;

    if (activeCameras.has(cameraId)) {
      const cameraState = activeCameras.get(cameraId)!;
      if (cameraState.recordingStatus === 'recording') {
        await stopRecording(cameraId);
      }
      await stopStream(cameraId);

      setActiveCameras(prev => {
        const newMap = new Map(prev);
        newMap.delete(cameraId);
        const activeCameraIds = Array.from(newMap.keys());
        sessionStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(activeCameraIds));
        return newMap;
      });
      return;
    }

    if (activeCameras.size >= MAX_CAMERAS) {
      alert(`Maximum of ${MAX_CAMERAS} cameras can be displayed simultaneously.`);
      return;
    }

    setActiveCameras(prev => {
      const newMap = new Map(prev);
      newMap.set(cameraId, {
        camera,
        streamUrl: null,
        isLoadingStream: true,
        streamError: null,
        recordingStatus: 'idle',
        hasPTZ: false,
        checkingPTZ: false,
      });
      return newMap;
    });

    try {
      const data = await startStream(cameraId);
      // streamUrl returned by Rust should be a full URL like http://localhost:port/stream.m3u8
      const fullStreamUrl = data.streamUrl; 
      console.log(`Stream process started. Polling for manifest at: ${fullStreamUrl}`);

      await pollForStream(fullStreamUrl);

      setActiveCameras(prev => {
        const newMap = new Map(prev);
        const cameraState = newMap.get(cameraId);
        if (cameraState) {
          newMap.set(cameraId, {
            ...cameraState,
            streamUrl: fullStreamUrl,
            isLoadingStream: false,
          });
        }
        const activeCameraIds = Array.from(newMap.keys());
        sessionStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(activeCameraIds));
        return newMap;
      });

      setActiveCameras(prev => {
        const newMap = new Map(prev);
        const cameraState = newMap.get(cameraId);
        if (cameraState) {
          newMap.set(cameraId, { ...cameraState, checkingPTZ: true });
        }
        return newMap;
      });

      try {
        const ptzCapabilities = await checkPTZCapabilities(cameraId);
        setActiveCameras(prev => {
          const newMap = new Map(prev);
          const cameraState = newMap.get(cameraId);
          if (cameraState) {
            newMap.set(cameraId, {
              ...cameraState,
              hasPTZ: ptzCapabilities.supported,
              checkingPTZ: false,
            });
          }
          return newMap;
        });
      } catch (ptzError: any) {
        console.error('Failed to check PTZ capabilities:', ptzError);
        setActiveCameras(prev => {
          const newMap = new Map(prev);
          const cameraState = newMap.get(cameraId);
          if (cameraState) {
            newMap.set(cameraId, { ...cameraState, hasPTZ: false, checkingPTZ: false });
          }
          return newMap;
        });
      }

        } catch (error) {

          console.error('Failed to start or poll for stream:', error);

          // Tauri invoke returns the error as a string directly in the catch block

          const errorMessage = error instanceof Error ? error.message : String(error);

    

          setActiveCameras(prev => {

            const newMap = new Map(prev);

    
        const cameraState = newMap.get(cameraId);
        if (cameraState) {
          newMap.set(cameraId, {
            ...cameraState,
            streamError: `Failed to start stream. ${errorMessage}`,
            isLoadingStream: false,
          });
        }
        return newMap;
      });
    }
  };

  const handleStartRecording = async (cameraId: number) => {
    try {
      await startRecording(cameraId);
      setActiveCameras(prev => {
        const newMap = new Map(prev);
        const cameraState = newMap.get(cameraId);
        if (cameraState) {
          newMap.set(cameraId, { ...cameraState, recordingStatus: 'recording' });
        }
        return newMap;
      });
    } catch (error) {
      console.error('Failed to start recording:', error);
    }
  };

  const handleStopRecording = async (cameraId: number) => {
    try {
      await stopRecording(cameraId);
      setActiveCameras(prev => {
        const newMap = new Map(prev);
        const cameraState = newMap.get(cameraId);
        if (cameraState) {
          newMap.set(cameraId, { ...cameraState, recordingStatus: 'idle' });
        }
        return newMap;
      });
      setRecordingListVersion(v => v + 1);
    } catch (error) {
      console.error('Failed to stop recording:', error);
    }
  };

  const handlePlayRecording = (filename: string) => {
    // In Tauri, we might need a custom protocol to serve files or localhost server.
    // Assuming the backend serves recordings at a specific URL.
    // We will use a placeholder or assume backend returns full URL or we construct it.
    // For now, let's assume Rust serves it at http://localhost:PORT/recordings/filename
    // I'll make this dynamic later.
    // Actually, `VideoPlayer` might expect a URL.
    const url = `http://localhost:3333/recordings/${filename}`; // TODO: Fix this port assumption
    setPlayingRecordingUrl(url);
    setIsPlaybackModalOpen(true);
  };

  const handleClosePlaybackModal = () => {
    setIsPlaybackModalOpen(false);
    setPlayingRecordingUrl(null);
  };

  const handleCameraAdded = () => {
    fetchCameras();
  };

  const handleCameraDeleted = async (deletedCameraId: number) => {
    if (activeCameras.has(deletedCameraId)) {
      const cameraState = activeCameras.get(deletedCameraId)!;
      if (cameraState.recordingStatus === 'recording') {
        await stopRecording(deletedCameraId);
      }
      await stopStream(deletedCameraId);
      setActiveCameras(prev => {
        const newMap = new Map(prev);
        newMap.delete(deletedCameraId);
        return newMap;
      });
    }
    fetchCameras();
  };


  return (
    <div className="min-h-screen bg-gray-100 pb-8">
      <CssBaseline />
      <AppBar position="static" className="bg-blue-600">
        <Toolbar>
          <Typography variant="h6" component="div" className="flex-grow font-semibold">
            ONVIF Camera Viewer (Tauri)
          </Typography>
          <IconButton
            color="inherit"
            onClick={() => setIsEncoderSettingsOpen(true)}
            title="Encoder Settings"
          >
            <SettingsIcon />
          </IconButton>
        </Toolbar>
      </AppBar>
      <main>
        <Container maxWidth="xl" className="py-8">
          <div className="flex justify-between items-center mb-6">
            <Typography variant="h4" component="h1" className="text-gray-800 font-bold">
              Cameras
            </Typography>
            <div className="flex gap-4">
              <Button variant="outlined" onClick={() => setIsDiscoverModalOpen(true)} className="bg-white">
                Discover Cameras
              </Button>
              <Button variant="contained" onClick={() => setIsAddCameraModalOpen(true)} className="bg-blue-600">
                Add Camera
              </Button>
            </div>
          </div>
          
          <CameraList
            cameras={cameras}
            loading={camerasLoading}
            error={camerasError}
            activeCameraIds={Array.from(activeCameras.keys())}
            onSelectCamera={handleSelectCamera}
            onCameraDeleted={handleCameraDeleted}
          />

          {activeCameras.size > 0 && (
            <div className="mt-8">
              <Typography variant="h5" component="h2" gutterBottom className="text-gray-700 font-medium border-b pb-2 mb-4">
                Live Streams ({activeCameras.size}/{MAX_CAMERAS})
              </Typography>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {Array.from(activeCameras.entries()).map(([cameraId, cameraState]) => (
                  <div
                    key={cameraId}
                    className="bg-white p-4 rounded-lg shadow-md border border-gray-200"
                  >
                    <div className="flex justify-between items-center mb-2">
                      <Typography variant="h6" component="h3" className="font-medium">
                        {cameraState.camera.name}
                      </Typography>
                      <Button
                        size="small"
                        variant="outlined"
                        color="error"
                        onClick={() => handleSelectCamera(cameraState.camera)}
                      >
                        Close
                      </Button>
                    </div>

                    {cameraState.isLoadingStream ? (
                      <div className="flex justify-center items-center h-[300px] bg-black/5 rounded">
                        <CircularProgress />
                      </div>
                    ) : cameraState.streamError ? (
                      <Alert severity="error">{cameraState.streamError}</Alert>
                    ) : cameraState.streamUrl ? (
                      <>
                        <VideoPlayer streamUrl={cameraState.streamUrl} />
                        <div className="mt-4 flex items-center gap-4 flex-wrap">
                          {cameraState.recordingStatus === 'idle' ? (
                            <Button
                              variant="contained"
                              color="primary"
                              size="small"
                              onClick={() => handleStartRecording(cameraId)}
                              className="bg-blue-600"
                            >
                              Start Recording
                            </Button>
                          ) : (
                            <Button
                              variant="contained"
                              color="secondary"
                              size="small"
                              onClick={() => handleStopRecording(cameraId)}
                              className="bg-red-600"
                            >
                              Stop Recording
                            </Button>
                          )}
                          {cameraState.recordingStatus === 'recording' && (
                            <div className="flex items-center gap-2 animate-pulse">
                              <div className="w-3 h-3 rounded-full bg-red-600"></div>
                              <Typography variant="body2" className="text-red-600 font-bold">REC</Typography>
                            </div>
                          )}
                        </div>
                        {cameraState.checkingPTZ ? (
                          <div className="mt-2 flex items-center gap-2">
                            <CircularProgress size={16} />
                            <Typography variant="caption">Checking PTZ...</Typography>
                          </div>
                        ) : cameraState.hasPTZ ? (
                          <PTZControls cameraId={cameraId} />
                        ) : (
                          <Paper elevation={3} sx={{ p: 3, mt: 2, backgroundColor: '#f9fafb', textAlign: 'center', color: 'text.secondary' }}>
                             <Typography variant="body2">PTZ Not Available</Typography>
                          </Paper>
                        )}
                      </>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          )}

          <RecordingList listVersion={recordingListVersion} onPlayRecording={handlePlayRecording} />

          <ScheduleRecording />

        </Container>
      </main>
      <AddCameraModal
        open={isAddCameraModalOpen}
        onClose={() => setIsAddCameraModalOpen(false)}
        onCameraAdded={handleCameraAdded}
      />
      <DiscoverCamerasModal
        open={isDiscoverModalOpen}
        onClose={() => setIsDiscoverModalOpen(false)}
        onCameraAdded={handleCameraAdded}
        registeredCameras={cameras}
      />
      <Modal
        open={isPlaybackModalOpen}
        onClose={handleClosePlaybackModal}
        aria-labelledby="recording-playback-modal"
      >
        <div className={modalClassName}>
          {playingRecordingUrl && (
            <video src={playingRecordingUrl} controls autoPlay className="w-full max-h-[70vh] bg-black" />
          )}
        </div>
      </Modal>
      <EncoderSettings
        open={isEncoderSettingsOpen}
        onClose={() => setIsEncoderSettingsOpen(false)}
      />
    </div>
  );
}

export default App;