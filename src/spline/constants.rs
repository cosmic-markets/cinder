//! Shared literals for the SOL spline TUI.

use ratatui::style::Color;

pub const SOL_SYMBOL: &str = "SOL";
pub const MIN_SOL_SPREAD_USD: f64 = 0.01;
pub const QUOTE_LOT_DECIMALS: i32 = 6;
/// Maximum number of price history points to keep for the chart.
pub const MAX_PRICE_HISTORY: usize = 150;
/// Number of top orderbook rows to display in the UI.
pub const TOP_N: usize = 5;

// Color Palette
pub const FIRE_ORANGE: Color = Color::Rgb(255, 165, 90);
pub const ASK_BORDER: Color = Color::Rgb(100, 40, 40);
pub const BID_BORDER: Color = Color::Rgb(40, 100, 40);
/// Pre-configured list of fast-selection preset order sizes.
pub const ORDER_SIZE_PRESETS: &[f64] = &[
    0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0,
    10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 750.0, 1_000.0, 2_500.0, 5_000.0, 10_000.0, 25_000.0,
    50_000.0, 100_000.0,
];
/// The default index in the preset list (0.1 SOL).
pub const DEFAULT_SIZE_INDEX: usize = 12;
