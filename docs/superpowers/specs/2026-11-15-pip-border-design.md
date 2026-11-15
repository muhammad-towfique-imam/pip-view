# PiP border option — Design

Date: 2026-11-15

## Intent

The user can put a colored border on the PiP window, or no border.

## Goals

- Border options: None, White, Green, Red, Blue, Yellow.
- The user selects the option in a "Border" submenu of the PiP right-click
  menu, between "Select camera" and "Exit". The options are radio buttons;
  the current option has a mark.
- The border is a solid 3 px line at the window edge, drawn on top of the
  image. The window size does not change. With letterbox bars, the line is
  at the window edge, not at the image edge.
- The app saves the option in `config.json` and uses it at the next start.

## Non-goals

- Custom colors or custom line widths.
- A border in the setup modes.

## Constraints

- An existing `config.json` without the border field must still load. The
  missing field gives None (`#[serde(default)]`). Otherwise the user must do
  the setup again.

## Approaches

Only 1 sensible way: an enum saved in the config and drawn with the egui
painter in the PiP UI. Other ways (an OS window frame, a separate overlay
window) add work and do not give a colored line. The user approved this
design.

## Left to the builder

- The exact RGB values of the colors (use the standard egui `Color32`
  constants).
