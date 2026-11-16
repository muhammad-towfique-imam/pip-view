//! Border option of the PiP window.

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

    /// The line color as (red, green, blue), or `None` for no border.
    pub fn rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Border::None => None,
            Border::White => Some((255, 255, 255)),
            Border::Green => Some((0, 255, 0)),
            Border::Red => Some((255, 0, 0)),
            Border::Blue => Some((0, 0, 255)),
            Border::Yellow => Some((255, 255, 0)),
        }
    }
}

/// An unknown value of any type (for example from a newer version) gives
/// `Border::None`, so that the rest of the config file still loads.
impl<'de> Deserialize<'de> for Border {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let name = value.as_str().unwrap_or_default();
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
    fn none_has_no_rgb() {
        assert_eq!(Border::None.rgb(), None);
    }

    #[test]
    fn rgb_values() {
        assert_eq!(Border::White.rgb(), Some((255, 255, 255)));
        assert_eq!(Border::Green.rgb(), Some((0, 255, 0)));
        assert_eq!(Border::Red.rgb(), Some((255, 0, 0)));
        assert_eq!(Border::Blue.rgb(), Some((0, 0, 255)));
        assert_eq!(Border::Yellow.rgb(), Some((255, 255, 0)));
    }

    #[test]
    fn default_is_none() {
        assert_eq!(Border::default(), Border::None);
    }
}
