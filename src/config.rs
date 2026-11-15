//! Saved settings: camera, area, and PiP window geometry.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::border::Border;
use crate::geometry::NormRect;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub camera_name: String,
    pub area: NormRect,
    pub window_pos: Option<(f32, f32)>,
    pub window_size: Option<(f32, f32)>,
    /// Missing in a v0.1.0 config file; then `Border::None`.
    #[serde(default)]
    pub border: Border,
}

/// `%APPDATA%\pip-view\config.json` on Windows.
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "pip-view").map(|d| d.config_dir().join("config.json"))
}

/// Reads the config. Returns `None` if the file is missing or not valid.
pub fn load(path: &Path) -> Option<Config> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes the config and creates the parent folder if necessary.
pub fn save(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

/// Returns the index of the first camera with the name `saved`.
pub fn find_camera(names: &[String], saved: &str) -> Option<usize> {
    names.iter().position(|n| n == saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::NormRect;
    use std::path::Path;

    fn sample() -> Config {
        Config {
            camera_name: "Logitech C920".into(),
            area: NormRect { x: 0.25, y: 0.5, w: 0.25, h: 0.125 },
            window_pos: Some((10.0, 20.0)),
            window_size: Some((320.0, 180.0)),
            border: Border::Red,
        }
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path), Some(sample()));
    }

    #[test]
    fn save_creates_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("config.json");
        save(&path, &sample()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn missing_file_is_none() {
        assert!(load(Path::new("/nonexistent/x/config.json")).is_none());
    }

    #[test]
    fn corrupt_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(load(&path).is_none());
    }

    /// The config format of v0.1.0, which has no "border" key.
    const V010_JSON: &str = r#"{
        "camera_name": "Logitech C920",
        "area": { "x": 0.25, "y": 0.5, "w": 0.25, "h": 0.125 },
        "window_pos": [10.0, 20.0],
        "window_size": [320.0, 180.0]
    }"#;

    #[test]
    fn old_config_without_border_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, V010_JSON).unwrap();
        let cfg = load(&path).expect("old config must load");
        assert_eq!(cfg.border, Border::None);
        assert_eq!(cfg.camera_name, "Logitech C920");
    }

    #[test]
    fn unknown_border_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let json = V010_JSON.replacen('{', r#"{ "border": "Purple","#, 1);
        std::fs::write(&path, json).unwrap();
        let cfg = load(&path).expect("config with unknown border must load");
        assert_eq!(cfg.border, Border::None);
    }

    #[test]
    fn non_string_border_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        for value in ["3", "null", "true", r#"{ "color": "red" }"#] {
            let json = V010_JSON.replacen('{', &format!(r#"{{ "border": {value},"#), 1);
            std::fs::write(&path, json).unwrap();
            let cfg = load(&path).unwrap_or_else(|| panic!("config with border {value} must load"));
            assert_eq!(cfg.border, Border::None, "border {value}");
        }
    }

    #[test]
    fn find_camera_by_name_not_index() {
        let names = vec!["OBS Virtual".to_string(), "Logitech C920".to_string()];
        assert_eq!(find_camera(&names, "Logitech C920"), Some(1));
        assert_eq!(find_camera(&names, "Gone Cam"), None);
    }
}
