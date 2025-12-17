# ONVIF Camera Viewer (Tauri)

This is a desktop application built with Tauri, designed to manage and view ONVIF-compliant IP cameras and generic RTSP cameras. It leverages a Rust backend for native capabilities and a React frontend for the user interface, incorporating Material Design principles with Tailwind CSS for styling.

## Features

-   **Multi-Camera Type Support**: Manage both ONVIF and RTSP cameras.
    *   **ONVIF Cameras**: Discover, add, and stream.
    *   **RTSP Cameras**: Add and stream.
-   **Camera Discovery**: Automatically discover ONVIF cameras on your local network using **Unicast WS-Discovery** (subnet scanning).
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
*   **ONVIF Protocol**: Custom SOAP implementation for `GetProfiles`, `GetStreamUri`, PTZ (`ContinuousMove`, `Stop`), and Time Sync (`GetSystemDateAndTime`, `SetSystemDateAndTime`).
*   **Video Processing**: [FFmpeg](https://ffmpeg.org/) for transcoding, recording, and thumbnail generation (requires system FFmpeg).
*   **Hardware Acceleration**: Automatic GPU detection and encoder selection (Intel QSV, NVIDIA NVENC, AMD AMF, VA-API, VideoToolbox).

## Getting Started

### Prerequisites

*   [Node.js](https://nodejs.org/) (v18 or later recommended)
*   [npm](https://www.npmjs.com/)
*   [Rust](https://www.rust-lang.org/tools/install) (with `rustup`)
*   [FFmpeg](https://ffmpeg.org/download.html) must be installed on your system and available in the system's PATH.
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
        -   `onvif.rs`: Custom ONVIF SOAP implementation
        -   `stream.rs`: FFmpeg streaming and recording control
        -   `gpu_detector.rs`: GPU hardware detection and encoder discovery
        -   `encoder.rs`: Encoder selection and configuration logic
        -   `lib.rs`: Application setup and initialization

## Current Status & Known Issues

*   **Discovery**: Unicast ONVIF device discovery is functional.
*   **Streaming**: Optimized for low-latency with proper keyframe handling. Supports up to 4 simultaneous camera streams.
    *   HLS.js configured for 30-second buffer with automatic error recovery.
    *   GPU-accelerated encoding automatically enabled when available.
    *   Keyframes forced every 2 seconds to prevent buffer holes.
*   **Recording**: Fully functional (Record/Stop/Play with automatic thumbnail generation).
*   **PTZ Control**: Implemented (Pan/Tilt/Zoom with UI feedback).
*   **Time Synchronization**: Fully implemented (GetSystemDateAndTime/SetSystemDateAndTime).
*   **Hardware Acceleration**: ✅ Fully implemented with automatic GPU detection and fallback.
    *   **CPU Usage Reduction**: ~70% reduction with GPU encoding (7-8% vs 20-30% per camera).
    *   **Supported Encoders**: Intel QSV, NVIDIA NVENC, AMD AMF, VA-API, VideoToolbox.
    *   **Automatic Fallback**: Falls back to CPU encoding (libx264) if GPU is unavailable.
    *   **Configuration**: Available via Encoder Settings (Settings icon ⚙️ in top-right corner of app bar)
        -   **Auto Mode**: Automatically uses GPU if available, falls back to CPU if GPU test fails
        -   **GPU Only Mode**: Forces GPU encoding (fails if GPU unavailable)
        -   **CPU Only Mode**: Always uses CPU encoding

## Contributing

Contributions are welcome! Whether it's reporting a bug, suggesting an enhancement, or submitting a pull request, your input is valued.

1.  **Fork the Project**
2.  **Create your Feature Branch** (`git checkout -b feature/AmazingFeature`)
3.  **Commit your Changes** (`git commit -m 'Add some AmazingFeature'`)
4.  **Push to the Branch** (`git push origin feature/AmazingFeature`)
5.  **Open a Pull Request**

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for more information.
