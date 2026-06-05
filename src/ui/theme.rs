use ratatui::style::{Color, Style};

pub const BG: Color = Color::Rgb(26, 27, 38); // #1a1b26 dark gray-blue
pub const SURFACE: Color = Color::Rgb(36, 37, 58); // #24253a
pub const SURFACE_HL: Color = Color::Rgb(41, 43, 66); // #292b42

pub const ACCENT: Color = Color::Rgb(125, 207, 255); // #7dcfff soft blue
pub const ACCENT_DIM: Color = Color::Rgb(86, 95, 137); // #565f89
pub const ACCENT_GLOW: Color = Color::Rgb(158, 206, 219); // #9ecedb

pub const TEXT: Color = Color::Rgb(192, 202, 245); // #c0caf5
pub const TEXT_DIM: Color = Color::Rgb(86, 95, 137); // #565f89
pub const TEXT_FAINT: Color = Color::Rgb(59, 66, 97); // #3b4261

pub const SUCCESS: Color = Color::Rgb(158, 206, 106); // #9ece6a soft green
pub const ERROR: Color = Color::Rgb(247, 118, 142); // #f7768e soft red
pub const DISCONNECTED: Color = Color::Rgb(86, 95, 137); // #565f89

pub const BORDER: Color = Color::Rgb(59, 66, 97); // #3b4261
pub const BORDER_ACTIVE: Color = ACCENT;

pub fn s_text() -> Style {
    Style::default().fg(TEXT)
}
pub fn s_dim() -> Style {
    Style::default().fg(TEXT_DIM)
}
pub fn s_faint() -> Style {
    Style::default().fg(TEXT_FAINT)
}
pub fn s_accent() -> Style {
    Style::default().fg(ACCENT)
}
pub fn s_accent_bold() -> Style {
    Style::default().fg(ACCENT_GLOW)
}
pub fn s_success() -> Style {
    Style::default().fg(SUCCESS)
}
pub fn s_error() -> Style {
    Style::default().fg(ERROR)
}
pub fn s_disconnected() -> Style {
    Style::default().fg(DISCONNECTED)
}
pub fn s_bg() -> Style {
    Style::default().bg(BG)
}
pub fn s_surface() -> Style {
    Style::default().bg(SURFACE)
}
