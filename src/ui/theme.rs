use ratatui::style::{Color, Style};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub bg: Color,
    pub surface: Color,
    pub accent: Color,
    pub accent_glow: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub success: Color,
    pub warn: Color,
    pub error: Color,
    pub disconnected: Color,
    pub border: Color,
    pub border_active: Color,
}

pub const THEMES: [Theme; 4] = [
    Theme {
        id: "graphite-dark",
        name: "Graphite Dark",
        bg: Color::Rgb(18, 18, 20),        // #121214 - deep rich graphite
        surface: Color::Rgb(32, 33, 37),   // #202125 - solid charcoal surface
        accent: Color::Rgb(220, 224, 230), // #dce0e6 - bright platinum silver
        accent_glow: Color::Rgb(255, 255, 255), // #ffffff - pure white highlight
        text: Color::Rgb(240, 242, 245),   // #f0f2f5 - bright crisp white
        text_dim: Color::Rgb(150, 155, 165), // #969ba5 - neutral graphite gray
        text_faint: Color::Rgb(95, 100, 110), // #5f646e - steel gray
        success: Color::Rgb(74, 222, 128), // #4ade80 - emerald green
        warn: Color::Rgb(251, 191, 36),    // #fbbf24 - amber
        error: Color::Rgb(248, 113, 113),  // #f87171 - coral red
        disconnected: Color::Rgb(105, 110, 120),
        border: Color::Rgb(46, 48, 54), // #2e3036 - slate border
        border_active: Color::Rgb(210, 215, 225), // #d2d7e1 - platinum active border
    },
    Theme {
        id: "graphite-light",
        name: "Graphite Light",
        bg: Color::Rgb(246, 247, 249), // #f6f7f9 - paper off-white background
        surface: Color::Rgb(230, 233, 238), // #e6e9ee - soft ash/slate surface
        accent: Color::Rgb(25, 26, 30), // #191a1e - dark ink black accent
        accent_glow: Color::Rgb(0, 0, 0), // #000000 - pure jet black
        text: Color::Rgb(24, 25, 28),  // #18191c - high contrast charcoal text
        text_dim: Color::Rgb(95, 102, 115), // #5f6673 - legible slate gray
        text_faint: Color::Rgb(145, 152, 163), // #9198a3 - pencil gray
        success: Color::Rgb(22, 163, 74), // #16a34a - forest green (high contrast on white)
        warn: Color::Rgb(217, 119, 6), // #d97706 - dark amber (high contrast on white)
        error: Color::Rgb(220, 38, 38), // #dc2626 - crimson red (high contrast on white)
        disconnected: Color::Rgb(130, 138, 150),
        border: Color::Rgb(205, 210, 218), // #cdd2da - light graphite border
        border_active: Color::Rgb(25, 26, 30), // #191a1e - bold ink active border
    },
    Theme {
        id: "tokyo-night",
        name: "Tokyo Night",
        bg: Color::Rgb(26, 27, 38),        // #1a1b26 - deep midnight blue
        surface: Color::Rgb(36, 40, 59),   // #24283b - elevated storm surface
        accent: Color::Rgb(122, 162, 247), // #7aa2f7 - Tokyo Night signature blue
        accent_glow: Color::Rgb(125, 207, 255), // #7dcfff - brilliant cyan highlight
        text: Color::Rgb(192, 202, 245),   // #c0caf5 - clean foreground
        text_dim: Color::Rgb(154, 165, 206), // #9aa5ce - crisp readable secondary
        text_faint: Color::Rgb(86, 95, 137), // #565f89 - distinct metadata/labels
        success: Color::Rgb(158, 206, 106), // #9ece6a - green
        warn: Color::Rgb(224, 175, 104),   // #e0af68 - warm amber
        error: Color::Rgb(247, 118, 142),  // #f7768e - coral red
        disconnected: Color::Rgb(86, 95, 137),
        border: Color::Rgb(59, 66, 97), // #3b4261 - distinct frame border
        border_active: Color::Rgb(125, 207, 255), // #7dcfff - cyan active border
    },
    Theme {
        id: "dracula",
        name: "Dracula",
        bg: Color::Rgb(40, 42, 54),        // #282a36 - Dracula background
        surface: Color::Rgb(68, 71, 90),   // #44475a - current line / selection surface
        accent: Color::Rgb(189, 147, 249), // #bd93f9 - Dracula purple
        accent_glow: Color::Rgb(255, 121, 198), // #ff79c6 - Dracula hot pink
        text: Color::Rgb(248, 248, 242),   // #f8f8f2 - Dracula foreground
        text_dim: Color::Rgb(164, 177, 214), // #a4b1d6 - readable comment highlight
        text_faint: Color::Rgb(98, 114, 164), // #6272a4 - Dracula comment
        success: Color::Rgb(80, 250, 123), // #50fa7b - electric green
        warn: Color::Rgb(241, 250, 140),   // #f1fa8c - yellow
        error: Color::Rgb(255, 85, 85),    // #ff5555 - red
        disconnected: Color::Rgb(98, 114, 164),
        border: Color::Rgb(68, 71, 90), // #44475a - selection border
        border_active: Color::Rgb(189, 147, 249), // #bd93f9 - purple active border
    },
];

static CURRENT_THEME_IDX: AtomicUsize = AtomicUsize::new(0);

pub fn current_theme() -> &'static Theme {
    let idx = CURRENT_THEME_IDX.load(Ordering::Relaxed);
    &THEMES[idx.min(THEMES.len() - 1)]
}

pub fn set_theme(id_or_name: &str) -> bool {
    if let Some(idx) = THEMES.iter().position(|t| {
        t.id.eq_ignore_ascii_case(id_or_name) || t.name.eq_ignore_ascii_case(id_or_name)
    }) {
        CURRENT_THEME_IDX.store(idx, Ordering::Relaxed);
        true
    } else {
        false
    }
}

pub fn cycle_theme() -> &'static str {
    let cur = CURRENT_THEME_IDX.load(Ordering::Relaxed);
    let next = (cur + 1) % THEMES.len();
    CURRENT_THEME_IDX.store(next, Ordering::Relaxed);
    THEMES[next].id
}

pub fn cycle_theme_prev() -> &'static str {
    let cur = CURRENT_THEME_IDX.load(Ordering::Relaxed);
    let prev = if cur == 0 { THEMES.len() - 1 } else { cur - 1 };
    CURRENT_THEME_IDX.store(prev, Ordering::Relaxed);
    THEMES[prev].id
}

#[allow(dead_code)]
pub fn available_themes() -> &'static [Theme] {
    &THEMES
}

pub fn bg() -> Color {
    current_theme().bg
}

pub fn surface() -> Color {
    current_theme().surface
}

pub fn accent() -> Color {
    current_theme().accent
}

pub fn accent_glow() -> Color {
    current_theme().accent_glow
}

pub fn text() -> Color {
    current_theme().text
}

pub fn text_dim() -> Color {
    current_theme().text_dim
}

pub fn text_faint() -> Color {
    current_theme().text_faint
}

pub fn success() -> Color {
    current_theme().success
}

pub fn warn() -> Color {
    current_theme().warn
}

pub fn error() -> Color {
    current_theme().error
}

pub fn disconnected() -> Color {
    current_theme().disconnected
}

pub fn border() -> Color {
    current_theme().border
}

pub fn border_active() -> Color {
    current_theme().border_active
}

pub fn traffic_down() -> Color {
    current_theme().success
}

pub fn traffic_up() -> Color {
    current_theme().error
}

pub fn s_text() -> Style {
    Style::default().fg(text())
}

pub fn s_dim() -> Style {
    Style::default().fg(text_dim())
}

pub fn s_faint() -> Style {
    Style::default().fg(text_faint())
}

pub fn s_accent() -> Style {
    Style::default().fg(accent())
}

pub fn s_accent_bold() -> Style {
    Style::default().fg(accent_glow())
}

pub fn s_success() -> Style {
    Style::default().fg(success())
}

pub fn s_warn() -> Style {
    Style::default().fg(warn())
}

pub fn s_error() -> Style {
    Style::default().fg(error())
}

pub fn s_disconnected() -> Style {
    Style::default().fg(disconnected())
}

pub fn s_bg() -> Style {
    Style::default().bg(bg()).fg(text())
}

pub fn s_surface() -> Style {
    Style::default().bg(surface()).fg(text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_selection_and_cycling() {
        assert_eq!(available_themes().len(), 4);
        assert_eq!(available_themes()[0].id, "graphite-dark");

        assert!(set_theme("dracula"));
        assert_eq!(current_theme().id, "dracula");
        assert_eq!(current_theme().name, "Dracula");
        assert_eq!(accent(), current_theme().accent);
        assert_eq!(border_active(), current_theme().border_active);

        let next = cycle_theme();
        assert_eq!(next, "graphite-dark");
        assert_eq!(current_theme().id, "graphite-dark");

        let next2 = cycle_theme();
        assert_eq!(next2, "graphite-light");
        assert_eq!(current_theme().id, "graphite-light");

        let prev = cycle_theme_prev();
        assert_eq!(prev, "graphite-dark");
        assert_eq!(current_theme().id, "graphite-dark");

        // Reset back to default
        assert!(set_theme("graphite-dark"));
        assert_eq!(current_theme().id, "graphite-dark");
    }
}
