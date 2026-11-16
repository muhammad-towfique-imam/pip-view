# webcam-view-pip — PiP Camera for Windows and Android

**A free picture-in-picture (PiP) webcam viewer for Windows and Android.**
webcam-view-pip shows your camera in a small window that stays on top of all
other apps. You can crop the camera image, so the window shows only the area
that you select.

- [**Download webcam-view-pip.exe**](https://github.com/muhammad-towfique-imam/webcam-view-pip/releases/latest/download/webcam-view-pip.exe)
  for Windows 10 and 11, 64-bit, no installation.
- [**Download webcam-view-pip.apk**](https://github.com/muhammad-towfique-imam/webcam-view-pip/releases/latest/download/webcam-view-pip.apk)
  for Android 8.0 and later.

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

## Use on Windows

1. Start `webcam-view-pip.exe`.
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
`%APPDATA%\webcam-view-pip\config.json`. At the next start, the app opens the PiP
window directly. If the saved camera is not connected, the app opens the
camera list. To start the setup again, delete the config file.

### If the Windows app freezes

The app writes a log to `%APPDATA%\webcam-view-pip\log.txt`. The log shows
the camera formats, the GPU, the time of each step, and the step where the
window stopped (lines with `UI STALL`). The app starts a new log at each start,
so copy the file before you start the app again. Send the file with a problem
report.

To use OpenGL in place of the default renderer, add `"renderer": "glow",` after
the first `{` in `%APPDATA%\webcam-view-pip\config.json`.

## Use on Android

1. Install `webcam-view-pip.apk`. Android asks you to allow installs from
   your browser or file app.
2. Start the app and allow the camera permission.
3. Select a camera and tap **Next**.
4. Drag on the camera image to select the area. Tap **Confirm**.
5. Allow **Display over other apps** for webcam-view-pip, then go back.
6. The PiP window opens on top of other apps.

In the PiP window:

- Drag the window to move it.
- Drag the bottom-right corner to resize it.
- Tap the window to open the menu: **Select area**, **Select camera**,
  **Border**, **Exit**.

While the PiP window is open, Android shows a notification. Tap **Exit** in
the notification to close the window. At the next start, the app opens the
PiP window directly with the saved settings.

## Repository layout

- `rust/` — the Windows app (Rust, egui).
- `android/` — the Android app (Kotlin, CameraX).
- `site/` — the website.

## Build

GitHub Actions builds both apps for each pull request and each manual run.
The `.exe` and the `.apk` are attached to the workflow run as artifacts.

To publish a release, push a version tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The workflow then attaches `webcam-view-pip.exe` and `webcam-view-pip.apk` to
a GitHub release for that tag. The tag also sets the Android version name.

### Android release key

Android installs an update over an earlier version only if both use the same
key. The certificate SHA-256 of the webcam-view-pip key is
`fd5a376ac527c757f975f9897effbe116811d2eca6b7fdfa7796c5e97158cbe3`.

Git contains the key and its passwords only as an encrypted file,
`android/keystore-backup/webcam-view-pip-key.enc` (AES-256-GCM, key from the
passphrase with PBKDF2). The release build decrypts it with the passphrase
from the environment variable `WEBCAMVIEWPIP_KEY_PASSPHRASE`, or from the
git-ignored file `android/.env` (see `android/.env.example`). Without a
passphrase, the release APK is unsigned. With a wrong passphrase, the build
stops.

The workflow gets the passphrase from the repository secret
`WEBCAMVIEWPIP_KEY_PASSPHRASE`. A tag build stops if the secret is missing or
if the APK has a different signer.

**CAUTION: Keep the passphrase in a password manager. Without it, the
encrypted file is useless. Never commit `android/.env`, `android/keystore/`,
or `android/keystore.properties`.**

To make the encrypted file again after a key change, write
`keystore.properties` and the passphrase in `android/.env`, then run
`./gradlew encryptReleaseKey` in `android/` and commit the new file.

Local build of the Windows app (Linux uses V4L2 for the camera, Windows uses
Media Foundation):

```sh
cd rust
cargo test
cargo build --release
```

Local build of the Android app (JDK 17 or later and the Android SDK):

```sh
cd android
./gradlew testDebugUnitTest assembleDebug
```

## License

MIT. See [LICENSE](LICENSE).
