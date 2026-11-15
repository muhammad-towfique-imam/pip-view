# PiP Border Option Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A saved border option (None, White, Green, Red, Blue, Yellow) for the PiP window, selected from the right-click menu.

**Architecture:** A `Border` enum in a new `src/border.rs` gives labels and colors. `Config` stores it with a serde default. `ui_pip` in `src/app.rs` adds a "Border" submenu and paints a 3 px line at the window edge.

**Tech Stack:** Rust, eframe/egui 0.37, serde.

**Spec:** `docs/superpowers/specs/2026-11-15-pip-border-design.md`

## Global Constraints

- Options and menu labels, in this order: "None", "White", "Green", "Red", "Blue", "Yellow". Default: None.
- Submenu label "Border", between "Select camera" and "Exit".
- Line: solid, 3 physical pixels wide, inside the window edge, on top of the image. The window size does not change.
- An existing `config.json` without a `border` field must load, with border None.

## Review Focus

1. Old `config.json` without `border` → loads, border None, PiP opens directly. Test in Task 1.
2. `config.json` with an unknown border value (for example from a newer version) → the config still loads, border None. Test in Task 1.
3. Letterbox bars → the line is at the window edge, not at the image edge. Checked by reading Task 2 code.
4. Display scale 150% → the line is still 3 physical pixels (stroke width `3.0 / pixels_per_point`). Checked by reading Task 2 code.
5. A border change → saved at once, so a process kill does not lose it. Checked by reading Task 2 code.

---

### Task 1: Border type and config field

**Files:**
- Create: `src/border.rs`; Modify: `src/main.rs` (`mod border;`), `src/config.rs`

**Interfaces:**
- Produces (in `src/border.rs`):
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] pub enum Border { #[default] #[serde(other)] None, White, Green, Red, Blue, Yellow }`
  - `impl Border { pub const ALL: [Border; 6]; pub fn label(self) -> &'static str; pub fn color(self) -> Option<egui::Color32> }` — `color` gives `None` for `Border::None`, else the egui constants `WHITE`, `GREEN`, `RED`, `BLUE`, `YELLOW`.
  - `pub const BORDER_PX: f32 = 3.0;`
- Produces (in `src/config.rs`): `Config` gets `#[serde(default)] pub border: Border`.

- [ ] **Step 1: Write the failing tests**

In `src/border.rs`:
```rust
#[test] fn all_in_menu_order() {
    let labels: Vec<_> = Border::ALL.iter().map(|b| b.label()).collect();
    assert_eq!(labels, ["None", "White", "Green", "Red", "Blue", "Yellow"]);
}
#[test] fn none_has_no_color() { assert_eq!(Border::None.color(), None); }
#[test] fn colors() {
    assert_eq!(Border::White.color(), Some(Color32::WHITE));
    assert_eq!(Border::Green.color(), Some(Color32::GREEN));
    assert_eq!(Border::Red.color(), Some(Color32::RED));
    assert_eq!(Border::Blue.color(), Some(Color32::BLUE));
    assert_eq!(Border::Yellow.color(), Some(Color32::YELLOW));
}
#[test] fn default_is_none() { assert_eq!(Border::default(), Border::None); }
```
In `src/config.rs` (and add `border: Border::Red` to `sample()`):
```rust
#[test] fn old_config_without_border_loads_as_none() {
    // write the JSON of the v0.1.0 format (no "border" key); load → Some, border == Border::None
}
#[test] fn unknown_border_loads_as_none() {
    // same JSON with "border": "Purple"; load → Some, border == Border::None
}
```
The existing `save_then_load_round_trip` then also covers a saved `Red`.

- [ ] **Step 2: Run tests, expect FAIL** — `cargo test` → compile errors (`Border` not defined).
- [ ] **Step 3: Implement the interface above.** If `#[serde(other)]` is not accepted on this enum, use a custom `Deserialize` that maps an unknown string to `None`.
- [ ] **Step 4: Run tests, expect PASS** — `cargo test` → 33 passed.
- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat: add border option to config"`

### Task 2: Border menu and drawing

**Files:**
- Modify: `src/app.rs`

**Interfaces:**
- Consumes: `Border`, `Border::ALL`, `label`, `color`, `BORDER_PX`, `Config::border` from Task 1.

- [ ] **Step 1: State** — `PipApp` gets `border: Border`. `new` reads it from the loaded config; `save_config` writes it.
- [ ] **Step 2: Drawing** — in `ui_pip`, after the image and before the grip mark: if `self.border.color()` is `Some(c)`, call `ui.painter().rect_stroke(full, 0.0, Stroke::new(BORDER_PX / ui.ctx().pixels_per_point(), c), StrokeKind::Inside)`. `full` is the whole window rect, so the line is at the window edge also with letterbox bars.
- [ ] **Step 3: Menu** — in the context menu, between "Select camera" and "Exit": `ui.menu_button("Border", |ui| for b in Border::ALL { if ui.radio(self.border == b, b.label()).clicked() { self.border = b; self.save_config(); } })`. If egui 0.37 renames `menu_button` or `radio`, use the 0.37 name.
- [ ] **Step 4: Verify** — `cargo clippy --all-targets -- -D warnings` clean; `cargo test` → 33 passed; run the app 5 s with `timeout 5 ./target/debug/pip-view` → exit 124, no panic.
- [ ] **Step 5: README** — add "Border" to the menu list in the Use section.
- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat: add border submenu to the PiP window"`
