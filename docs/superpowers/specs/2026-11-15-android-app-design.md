# Android app and monorepo — Design

Date: 2026-11-15

## Intent

The user wants an Android version of webcam-view-pip in the same repository.
The Windows app moves to `rust/`, and the Android app goes in `android/`.

## Goals

- The same flow as the Windows app: select a camera, select an area, then
  show only that area in a small window on top of other apps.
- The same PiP window controls: drag to move, drag the bottom-right corner to
  resize with the aspect kept, and a menu with Select area, Select camera,
  Border (None, White, Green, Red, Blue, Yellow), and Exit.
- The app saves the settings and opens the PiP window directly at the next
  start.
- GitHub Actions builds the APK and attaches it to the same release as the
  Windows `.exe`.

## Non-goals

- A local release build. The release comes from the workflow.
- Old config support (the user said so for the Windows app rename).

## Approaches

The user said "start" and gave the choice of the method to the builder.

1. **Floating overlay window (selected).** A foreground service of type
   camera shows a `TYPE_APPLICATION_OVERLAY` window. It behaves like the
   Windows app: free position, free size, custom menu, custom border. The
   user must allow "Display over other apps".
2. **System PiP mode.** No extra permission, but Android controls the size,
   the position, and the aspect ratio limits, and there is no custom menu.
   It does not match the Windows app.

## Builder decisions

- Kotlin, CameraX 1.4, platform views (no Compose), minSdk 26, targetSdk 35.
- Application ID: `io.github.muhammadtowfiqueimam.webcamviewpip`.
- Crop: the `PreviewView` (TextureView mode) gets the size and offset of the
  full image, so the overlay window shows only the area (`innerBox`).
- The first PiP width is 45% of the short screen side. The window stays
  fully on the screen.
- The border is 3 dp wide, not 3 physical pixels, because phone screens have
  a high density.
- A tap on the PiP window opens the menu (Android has no right-click).
- The release tag sets `versionName`, and `versionCode` is
  `major*10000 + minor*100 + patch`.
- Release signing uses the same method as the led-masjid-clock repo (the
  user's choice): the keystore is in git as an AES-256-GCM encrypted file,
  and the passphrase is in the `WEBCAMVIEWPIP_KEY_PASSPHRASE` secret.

## Known limits

- The area is relative to the camera image as it shows in the current
  screen rotation. If the user rotates the phone, the area can show a
  different part of the image.
- Many phones do not support USB webcams through the Android camera API.
