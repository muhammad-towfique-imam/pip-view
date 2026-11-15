# pip-view — PiP Camera for Windows

**A free picture-in-picture (PiP) webcam viewer for Windows.** pip-view shows
your camera in a small window that stays on top of all other apps. You can
crop the camera image, so the window shows only the area that you select.

[**Download pip-view.exe**](https://github.com/muhammad-towfique-imam/webcam-view-pip/releases/latest/download/pip-view.exe)
(Windows 10 and 11, 64-bit, no installation) ·
[Website](https://muhammad-towfique-imam.github.io/webcam-view-pip/) ·
[All releases](https://github.com/muhammad-towfique-imam/webcam-view-pip/releases)

## Features

- Webcam window that is always on top, with no border.
- Select any area of the camera image, for example a whiteboard, a desk, a
  door, or a part of a second screen.
- Move the window by dragging it. Resize it from the bottom-right corner.
- Optional colored border: White, Green, Red, Blue, or Yellow.
- Works with USB webcams, laptop cameras, capture cards, and virtual cameras.
- The app remembers the camera, the area, and the window position.
- 1 small `.exe` file. Free and open source (MIT license).

## Use

1. Start `pip-view.exe`.
2. Select a camera and click **Next**.
3. Drag on the camera image to select the area. Click **Confirm**.
4. The PiP window opens.

In the PiP window:

- Drag the window to move it.
- Drag the bottom-right corner to resize it.
- Right-click to open the menu: **Select area**, **Select camera**,
  **Border**, **Exit**.
- **Border** puts a colored line at the window edge: None, White, Green, Red,
  Blue, or Yellow. The app saves the selection.

The app saves the camera, the area, and the window position in
`%APPDATA%\pip-view\config.json`. At the next start, the app opens the PiP
window directly. If the saved camera is not connected, the app opens the
camera list. To start the setup again, delete the config file.

## Build

GitHub Actions builds the app on Windows for each pull request and each
manual run. The `.exe` is attached to the workflow run as an artifact.

To publish a release, push a version tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The workflow then attaches `pip-view.exe` to a GitHub release for that tag.

Local build (Linux uses V4L2 for the camera, Windows uses Media Foundation):

```sh
cargo test
cargo build --release
```

## License

MIT. See [LICENSE](LICENSE).
