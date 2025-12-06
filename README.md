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
-   **Playback**: Built-in video player to view your recorded clips.
-   **PTZ Control**: Control Pan, Tilt, and Zoom for supported ONVIF cameras directly from the application.
    *   Includes intuitive UI for continuous movement controls.
    *   Displays status for non-PTZ cameras.
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
*   **ONVIF Protocol**: Custom SOAP implementation for `GetProfiles`, `GetStreamUri`, and PTZ (`ContinuousMove`, `Stop`).
*   **Video Processing**: [FFmpeg](https://ffmpeg.org/) for transcoding and recording (requires system FFmpeg).

## Getting Started

### Prerequisites

*   [Node.js](https://nodejs.org/) (v18 or later recommended)
*   [npm](https://www.npmjs.com/)
*   [Rust](https://www.rust-lang.org/tools/install) (with `rustup`)
*   [FFmpeg](https://ffmpeg.org/download.html) must be installed on your system and available in the system's PATH.

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
-   `/src-tauri`: Backend (Rust) source code and Tauri configuration.
    -   `/src-tauri/src`: Rust modules (db, models, commands, onvif, stream).

## Current Status & Known Issues

*   **Discovery**: Unicast ONVIF device discovery is functional.
*   **Streaming**: Stable. Transcoding to H.264 ensures wide compatibility.
*   **Recording**: Fully functional (Record/Stop/Play).
*   **PTZ Control**: Implemented (Pan/Tilt/Zoom with UI feedback).
*   **Time Synchronization**: Not yet implemented.

## Contributing

Contributions are welcome! Whether it's reporting a bug, suggesting an enhancement, or submitting a pull request, your input is valued.

1.  **Fork the Project**
2.  **Create your Feature Branch** (`git checkout -b feature/AmazingFeature`)
3.  **Commit your Changes** (`git commit -m 'Add some AmazingFeature'`)
4.  **Push to the Branch** (`git push origin feature/AmazingFeature`)
5.  **Open a Pull Request**

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for more information.
