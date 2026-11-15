//! Border option of the PiP window.

use eframe::egui::Color32;
use serde::{Deserialize, Deserializer, Serialize};

/// Width of the border line in physical pixels.
pub const BORDER_PX: f32 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Border {
    #[default]
    None,
    White,
    Green,
    Red,
    Blue,
    Yellow,
}

impl Border {
    /// All options, in menu order.
    pub const ALL: [Border; 6] =
        [Border::None, Border::White, Border::Green, Border::Red, Border::Blue, Border::Yellow];

    /// The menu label.
    pub fn label(self) -> &'static str {
        match self {
            Border::None => "None",
            Border::White => "White",
            Border::Green => "Green",
            Border::Red => "Red",
            Border::Blue => "Blue",
            Border::Yellow => "Yellow",
        }
    }

    /// The line color, or `None` for no border.
    pub fn color(self) -> Option<Color32> {
        match self {
            Border::None => None,
            Border::White => Some(Color32::WHITE),
            Border::Green => Some(Color32::GREEN),
            Border::Red => Some(Color32::RED),
            Border::Blue => Some(Color32::BLUE),
            Border::Yellow => Some(Color32::YELLOW),
        }
    }
}

/// An unknown value (for example from a newer version) gives `Border::None`,
/// so that the rest of the config file still loads.
impl<'de> Deserialize<'de> for Border {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(Border::ALL.into_iter().find(|b| b.label() == name).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_in_menu_order() {
        let labels: Vec<_> = Border::ALL.iter().map(|b| b.label()).collect();
        assert_eq!(labels, ["None", "White", "Green", "Red", "Blue", "Yellow"]);
    }

    #[test]
    fn none_has_no_color() {
        assert_eq!(Border::None.color(), None);
    }

    #[test]
    fn colors() {
        assert_eq!(Border::White.color(), Some(Color32::WHITE));
        assert_eq!(Border::Green.color(), Some(Color32::GREEN));
        assert_eq!(Border::Red.color(), Some(Color32::RED));
        assert_eq!(Border::Blue.color(), Some(Color32::BLUE));
        assert_eq!(Border::Yellow.color(), Some(Color32::YELLOW));
    }

    #[test]
    fn default_is_none() {
        assert_eq!(Border::default(), Border::None);
    }
}
