# ONVIF Camera Viewer (Tauri)

This is a desktop application built with Tauri, designed to manage and view ONVIF-compliant IP cameras and generic RTSP cameras. It leverages a Rust backend for native capabilities and a React frontend for the user interface, incorporating Material Design principles with Tailwind CSS for styling.

## Features

-   **Multi-Camera Type Support**: Manage ONVIF, RTSP, and UVC cameras.
    *   **ONVIF Cameras**: Discover, add, and stream with PTZ control and time sync.
    *   **RTSP Cameras**: Add and stream generic RTSP cameras.
    *   **UVC Cameras** 🆕: USB Video Class (webcam) support with automatic format detection.
-   **Camera Discovery**: Automatically discover cameras on your local network and system.
    *   **ONVIF**: Unicast WS-Discovery (subnet scanning).
    *   **UVC**: Automatic detection of USB webcams (Linux v4l2, Windows DirectShow, macOS AVFoundation).
-   **Camera Management**: Register, update, delete, and list cameras.
-   **Live Streaming**: View live HLS streams from cameras. FFmpeg handles RTSP to HLS transcoding (H.264/AAC) on the backend to ensure compatibility with modern browsers.
-   **Recording**: Record live streams directly to your local disk.
    *   Safely records to `.ts` format and automatically remuxes to `.mp4` upon completion.
    *   **Automatic Thumbnails**: Generates thumbnails from recorded videos for easy preview.
-   **Playback**: Built-in video player to view your recorded clips with thumbnail previews.
-   **PTZ Control**: Control Pan, Tilt, and Zoom for supported ONVIF cameras directly from the application.
    *   Includes intuitive UI for continuous movement controls.
    *   Displays status for non-PTZ cameras.
-   **Time Synchronization**: Synchronize ONVIF camera time with server time.
    *   Automatic sync when adding new cameras.
    *   Manual sync via UI button for existing cameras.
    *   Displays time difference information.
-   **Hardware Acceleration**: Automatic GPU encoder detection and selection for optimal performance.
    *   Supports Intel QSV, NVIDIA NVENC, AMD AMF, VA-API, and VideoToolbox.
    *   Reduces CPU usage by up to 70% compared to software encoding.
    *   Automatic fallback to CPU encoding if GPU is unavailable.
-   **UVC Camera Optimization** 🆕: Intelligent format and FPS detection for webcams.
    *   **Auto-Detection**: Automatically detects optimal video format (MJPEG preferred over raw YUYV).
    *   **Native FPS**: Streams at camera's native frame rate (no CPU-intensive resampling).
    *   **Dynamic Keyframes**: Keyframe interval adjusts based on camera FPS for consistent 2-second segments.
    *   **Multi-Resolution Support**: Automatically selects highest supported resolution and frame rate.
    *   **Platform Support**: Linux (v4l2), Windows (DirectShow), macOS (AVFoundation).
-   **Scheduled Recording** 🆕: Automate recording with flexible time-based scheduling.
    *   **Cron-based Scheduling**: Use cron expressions for flexible time patterns (e.g., daily at 9 AM, weekdays at 6 PM).
    *   **Visual Cron Builder**: Intuitive UI for building cron expressions without manual syntax.
    *   **Next Execution Display**: Shows the next scheduled recording time in real-time.
    *   **Active/Inactive Status**: Color-coded status indicators (green for active, gray for inactive).
    *   **FPS Control**: Specify custom frame rates for scheduled recordings.
    *   **Duration Control**: Set recording duration in minutes.
    *   **Enable/Disable Toggle**: Temporarily disable schedules without deletion.
    *   **Persistent Schedules**: Automatically resume enabled schedules after app restart.
    *   **Auto-Update Recording List**: Recording list automatically updates when recording completes (no manual reload needed).
    *   **JST Timezone**: All schedules use Japan Standard Time (Asia/Tokyo).
-   **Modern UI**: Built with React, Material Design principles, and styled with Tailwind CSS.

## Technology Stack

### Frontend
*   **Framework**: [React](https://reactjs.org/) with [Vite](https://vitejs.dev/)
*   **Language**: [TypeScript](https://www.typescriptlang.org/)
*   **UI Library**: [Material UI (MUI)](https://mui.com/) components with [Tailwind CSS](https://tailwindcss.com/) for styling.
*   **Video Playback**: [hls.js](https://github.com/video-dev/hls.js) for live streams, standard HTML5 video for recordings.

### Backend (Rust - Tauri Core)
*   **Language**: [Rust](https://www.rust-lang.org/)
*   **Database**: [SQLite3](https://www.sqlite.org/index.html) with `rusqlite` crate.
*   **Local Server**: [Axum](https://docs.rs/axum/latest/axum/) for serving HLS streams and recording files.
*   **Plugin Architecture**: Extensible camera plugin system supporting multiple camera types.
    *   **ONVIF Plugin**: Custom SOAP implementation for `GetProfiles`, `GetStreamUri`, PTZ, and Time Sync.
    *   **UVC Plugin**: USB Video Class camera support with v4l2/DirectShow/AVFoundation.
*   **Video Processing**: [FFmpeg](https://ffmpeg.org/) for transcoding, recording, and thumbnail generation (requires system FFmpeg).
*   **Hardware Acceleration**: Automatic GPU detection and encoder selection (Intel QSV, NVIDIA NVENC, AMD AMF, VA-API, VideoToolbox).
*   **Task Scheduling**: [tokio-cron-scheduler](https://crates.io/crates/tokio-cron-scheduler) with [croner](https://crates.io/crates/croner) for automated recording schedules with JST timezone support.

## Getting Started

### Prerequisites

*   [Node.js](https://nodejs.org/) (v18 or later recommended)
*   [npm](https://www.npmjs.com/)
*   [Rust](https://www.rust-lang.org/tools/install) (with `rustup`)
*   [FFmpeg](https://ffmpeg.org/download.html) must be installed on your system and available in the system's PATH.
*   **(Linux)** `v4l2-utils` for UVC camera detection:
    ```bash
    sudo apt install v4l-utils  # Ubuntu/Debian
    ```
*   **(Optional)** GPU drivers for hardware acceleration:
    *   **Intel**: `intel-media-va-driver`, `libva2`, `vainfo` (Linux)
    *   **NVIDIA**: Latest NVIDIA drivers with NVENC support
    *   **AMD**: Mesa drivers with AMF support (Linux)
    *   **macOS**: VideoToolbox (built-in)

### Installation

1.  **Clone the repository:**
    ```bash
    git clone [your-repo-url]
    cd try-tauri-app-onvif-cameras-viewer
    ```
2.  **Install Frontend dependencies:**
    ```bash
    npm install
    ```

### Running the Application (Development)

To run the application in development mode:

```bash
npm run tauri dev
```
This will start the Tauri application, including the Rust backend and the React frontend.

### Building for Production

To create a production-ready build:

```bash
npm run tauri build
```
The bundled application will be found in `src-tauri/target/release/bundle/`.

## Project Structure

-   `/src`: Frontend (React, TypeScript) source code.
    -   `/src/components`: React UI components
    -   `/src/services`: API layer for Tauri commands
-   `/src-tauri`: Backend (Rust) source code and Tauri configuration.
    -   `/src-tauri/src`: Rust modules
        -   `db.rs`: SQLite database operations
        -   `models.rs`: Data structures and types
        -   `commands.rs`: Tauri RPC command handlers
        -   `camera_plugin.rs`: Plugin architecture trait and plugin manager
        -   `/plugins`: Camera type implementations
            -   `onvif_plugin.rs`: ONVIF camera plugin with custom SOAP implementation
            -   `uvc_plugin.rs`: UVC (USB webcam) camera plugin with v4l2/DirectShow/AVFoundation
        -   `onvif.rs`: ONVIF SOAP protocol utilities
        -   `stream.rs`: FFmpeg streaming and recording control
        -   `scheduler.rs`: Cron-based recording schedule management
        -   `gpu_detector.rs`: GPU hardware detection and encoder discovery
        -   `encoder.rs`: Encoder selection and configuration logic
        -   `lib.rs`: Application setup and initialization

## Current Status & Known Issues

*   **Discovery**:
    *   **ONVIF**: Unicast device discovery is functional.
    *   **UVC**: Fully functional with automatic format/resolution/FPS detection.
*   **Streaming**: Optimized for low-latency with intelligent format handling.
    *   **Multi-Camera Support**: Supports up to 4 simultaneous camera streams.
    *   **HLS Configuration**: 10-second buffer with automatic error recovery.
    *   **GPU Acceleration**: Automatically enabled when available.
    *   **Dynamic Keyframes**: Keyframe interval adjusts per camera FPS (e.g., 10fps = 20 frames, 30fps = 60 frames) for consistent 2-second segments.
    *   **UVC Optimization**: Native FPS streaming (no resampling), MJPEG preferred over YUYV.
*   **Recording**: Fully functional (Record/Stop/Play with automatic thumbnail generation and real-time list updates).
*   **PTZ Control**: Implemented for ONVIF cameras (Pan/Tilt/Zoom with UI feedback).
*   **Time Synchronization**: Fully implemented for ONVIF cameras (GetSystemDateAndTime/SetSystemDateAndTime).
*   **Scheduled Recording**: Fully functional with cron-based automation and real-time UI updates.
*   **Hardware Acceleration**: Fully implemented with automatic GPU detection and fallback.
    *   **CPU Usage Reduction**: ~70% reduction with GPU encoding (7-8% vs 20-30% per camera).
    *   **Supported Encoders**: Intel QSV, NVIDIA NVENC, AMD AMF, VA-API, VideoToolbox.
    *   **Automatic Fallback**: Falls back to CPU encoding (libx264) if GPU is unavailable.
    *   **Configuration**: Available via Encoder Settings (Settings icon ⚙️ in top-right corner of app bar)
        -   **Auto Mode**: Automatically uses GPU if available, falls back to CPU if GPU test fails
        -   **GPU Only Mode**: Forces GPU encoding (fails if GPU unavailable)
        -   **CPU Only Mode**: Always uses CPU encoding
*   **UVC Camera Support** 🆕: Fully functional with automatic optimization.
    *   **Auto-Detection**: Format, resolution, and FPS detected via v4l2-ctl (Linux).
    *   **Metadata Filtering**: Automatically skips metadata-only devices.
    *   **Platform Support**: Linux (v4l2) fully tested, Windows/macOS detection stubs ready.
    *   **Performance**: No resampling overhead, optimal format selection (MJPEG > YUYV).
    *   **Smart Device Management**: Auto-stops streaming when recording starts (v4l2 exclusive access).

## Contributing

Contributions are welcome! Whether it's reporting a bug, suggesting an enhancement, or submitting a pull request, your input is valued.

1.  **Fork the Project**
2.  **Create your Feature Branch** (`git checkout -b feature/AmazingFeature`)
3.  **Commit your Changes** (`git commit -m 'Add some AmazingFeature'`)
4.  **Push to the Branch** (`git push origin feature/AmazingFeature`)
5.  **Open a Pull Request**

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for more information.
