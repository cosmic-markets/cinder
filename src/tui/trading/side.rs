//! Trading direction (long/short) and its display color.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingSide {
    Long,
    Short,
}

impl TradingSide {
    pub fn color(self) -> Color {
        match self {
            Self::Long => Color::LightGreen,
            Self::Short => Color::LightRed,
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Long => Self::Short,
            Self::Short => Self::Long,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_is_involution() {
        assert_eq!(TradingSide::Long.toggle(), TradingSide::Short);
        assert_eq!(TradingSide::Short.toggle(), TradingSide::Long);
        assert_eq!(TradingSide::Long.toggle().toggle(), TradingSide::Long);
    }

    #[test]
    fn color_distinguishes_sides() {
        assert_eq!(TradingSide::Long.color(), Color::LightGreen);
        assert_eq!(TradingSide::Short.color(), Color::LightRed);
    }
}
