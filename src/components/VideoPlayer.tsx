import React, { useEffect, useRef } from 'react';
import Hls from 'hls.js';

interface VideoPlayerProps {
  streamUrl: string;
}

const VideoPlayer: React.FC<VideoPlayerProps> = ({ streamUrl }) => {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    let hls: Hls | null = null;

    if (videoRef.current) {
      const video = videoRef.current;

      // hls.js is used for most browsers
      if (Hls.isSupported()) {
        const hlsConfig = {
          // Manifest loading retry settings
          manifestLoadingMaxRetry: 9,
          manifestLoadingRetryDelay: 1000,

          // Low latency live streaming optimizations
          liveSyncDurationCount: 1,        // Keep close to live edge (1 segment = minimal latency)
          liveMaxLatencyDurationCount: 5,  // Max latency before seeking back to live (reduced)
          maxBufferLength: 10,             // Max buffer size in seconds (reduced for low latency)
          maxMaxBufferLength: 20,          // Absolute max buffer (reduced)
          maxBufferSize: 20 * 1000 * 1000, // 20 MB max buffer size (reduced)

          // Improve segment loading for live streams
          manifestLoadingTimeOut: 10000,
          levelLoadingTimeOut: 10000,
          fragLoadingTimeOut: 20000,

          // Low latency mode
          backBufferLength: 10,            // Keep 10 seconds of back buffer (reduced for low latency)
        };
        hls = new Hls(hlsConfig);
        hls.loadSource(streamUrl);
        hls.attachMedia(video);
        hls.on(Hls.Events.MANIFEST_PARSED, () => {
          video.play().catch(e => console.error("Autoplay was prevented. Please click play.", e));
        });
        hls.on(Hls.Events.ERROR, (event, data) => {
            console.error("HLS Error:", event, data);
            // Auto-recover from non-fatal errors
            if (data.fatal) {
              switch (data.type) {
                case Hls.ErrorTypes.NETWORK_ERROR:
                  console.log("Network error, trying to recover...");
                  hls?.startLoad();
                  break;
                case Hls.ErrorTypes.MEDIA_ERROR:
                  console.log("Media error, trying to recover...");
                  hls?.recoverMediaError();
                  break;
                default:
                  console.error("Fatal error, cannot recover");
                  hls?.destroy();
                  break;
              }
            }
        });
      } 
      // Native HLS support in Safari
      else if (video.canPlayType('application/vnd.apple.mpegurl')) {
        video.src = streamUrl;
        video.addEventListener('loadedmetadata', () => {
          video.play().catch(e => console.error("Autoplay was prevented. Please click play.", e));
        });
      }
    }

    // Cleanup function to destroy hls instance on component unmount
    return () => {
      if (hls) {
        hls.destroy();
      }
    };
  }, [streamUrl]); // Re-run effect if streamUrl changes

  return (
    <video
      ref={videoRef}
      controls
      autoPlay
      muted // Autoplay on most browsers requires the video to be muted
      style={{ width: '100%', backgroundColor: '#000' }}
    />
  );
};

export default VideoPlayer;
