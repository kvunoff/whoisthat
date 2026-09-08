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

pub const THEMES: [Theme; 8] = [
    Theme {
        id: "tokyo-night",
        name: "Tokyo Night",
        bg: Color::Rgb(26, 27, 38),
        surface: Color::Rgb(36, 37, 58),
        accent: Color::Rgb(125, 207, 255),
        accent_glow: Color::Rgb(158, 206, 219),
        text: Color::Rgb(192, 202, 245),
        text_dim: Color::Rgb(86, 95, 137),
        text_faint: Color::Rgb(59, 66, 97),
        success: Color::Rgb(158, 206, 106),
        warn: Color::Rgb(224, 175, 104),
        error: Color::Rgb(247, 118, 142),
        disconnected: Color::Rgb(86, 95, 137),
        border: Color::Rgb(59, 66, 97),
        border_active: Color::Rgb(125, 207, 255),
    },
    Theme {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        bg: Color::Rgb(30, 30, 46),
        surface: Color::Rgb(49, 50, 68),
        accent: Color::Rgb(137, 180, 250),
        accent_glow: Color::Rgb(180, 190, 254),
        text: Color::Rgb(205, 214, 244),
        text_dim: Color::Rgb(166, 173, 200),
        text_faint: Color::Rgb(88, 91, 112),
        success: Color::Rgb(166, 227, 161),
        warn: Color::Rgb(249, 226, 175),
        error: Color::Rgb(243, 139, 168),
        disconnected: Color::Rgb(108, 112, 134),
        border: Color::Rgb(69, 71, 90),
        border_active: Color::Rgb(137, 180, 250),
    },
    Theme {
        id: "nord",
        name: "Nord",
        bg: Color::Rgb(46, 52, 64),
        surface: Color::Rgb(59, 66, 82),
        accent: Color::Rgb(136, 192, 208),
        accent_glow: Color::Rgb(129, 161, 193),
        text: Color::Rgb(236, 239, 244),
        text_dim: Color::Rgb(216, 222, 233),
        text_faint: Color::Rgb(76, 86, 106),
        success: Color::Rgb(163, 190, 140),
        warn: Color::Rgb(235, 203, 139),
        error: Color::Rgb(191, 97, 106),
        disconnected: Color::Rgb(76, 86, 106),
        border: Color::Rgb(67, 76, 94),
        border_active: Color::Rgb(136, 192, 208),
    },
    Theme {
        id: "dracula",
        name: "Dracula",
        bg: Color::Rgb(40, 42, 54),
        surface: Color::Rgb(56, 58, 89),
        accent: Color::Rgb(189, 147, 249),
        accent_glow: Color::Rgb(255, 121, 198),
        text: Color::Rgb(248, 248, 242),
        text_dim: Color::Rgb(98, 114, 164),
        text_faint: Color::Rgb(68, 71, 90),
        success: Color::Rgb(80, 250, 123),
        warn: Color::Rgb(241, 250, 140),
        error: Color::Rgb(255, 85, 85),
        disconnected: Color::Rgb(98, 114, 164),
        border: Color::Rgb(68, 71, 90),
        border_active: Color::Rgb(189, 147, 249),
    },
    Theme {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        bg: Color::Rgb(40, 40, 40),
        surface: Color::Rgb(60, 56, 54),
        accent: Color::Rgb(254, 128, 25),
        accent_glow: Color::Rgb(250, 189, 47),
        text: Color::Rgb(235, 219, 178),
        text_dim: Color::Rgb(168, 153, 132),
        text_faint: Color::Rgb(80, 73, 69),
        success: Color::Rgb(184, 187, 38),
        warn: Color::Rgb(250, 189, 47),
        error: Color::Rgb(251, 73, 52),
        disconnected: Color::Rgb(124, 111, 100),
        border: Color::Rgb(80, 73, 69),
        border_active: Color::Rgb(254, 128, 25),
    },
    Theme {
        id: "solarized-dark",
        name: "Solarized Dark",
        bg: Color::Rgb(0, 43, 54),
        surface: Color::Rgb(7, 54, 66),
        accent: Color::Rgb(38, 139, 210),
        accent_glow: Color::Rgb(42, 161, 152),
        text: Color::Rgb(131, 148, 150),
        text_dim: Color::Rgb(101, 123, 131),
        text_faint: Color::Rgb(88, 110, 117),
        success: Color::Rgb(133, 153, 0),
        warn: Color::Rgb(181, 137, 0),
        error: Color::Rgb(220, 50, 47),
        disconnected: Color::Rgb(88, 110, 117),
        border: Color::Rgb(7, 54, 66),
        border_active: Color::Rgb(38, 139, 210),
    },
    Theme {
        id: "cyberpunk",
        name: "Cyberpunk",
        bg: Color::Rgb(15, 15, 27),
        surface: Color::Rgb(26, 26, 46),
        accent: Color::Rgb(0, 255, 159),
        accent_glow: Color::Rgb(0, 184, 255),
        text: Color::Rgb(224, 224, 255),
        text_dim: Color::Rgb(138, 142, 168),
        text_faint: Color::Rgb(55, 55, 82),
        success: Color::Rgb(0, 255, 159),
        warn: Color::Rgb(255, 230, 0),
        error: Color::Rgb(255, 0, 85),
        disconnected: Color::Rgb(93, 93, 125),
        border: Color::Rgb(44, 44, 77),
        border_active: Color::Rgb(0, 255, 159),
    },
    Theme {
        id: "monokai-pro",
        name: "Monokai Pro",
        bg: Color::Rgb(45, 42, 46),
        surface: Color::Rgb(64, 62, 65),
        accent: Color::Rgb(255, 216, 102),
        accent_glow: Color::Rgb(252, 152, 103),
        text: Color::Rgb(252, 252, 250),
        text_dim: Color::Rgb(147, 146, 147),
        text_faint: Color::Rgb(91, 89, 92),
        success: Color::Rgb(169, 220, 118),
        warn: Color::Rgb(252, 152, 103),
        error: Color::Rgb(255, 97, 136),
        disconnected: Color::Rgb(114, 112, 114),
        border: Color::Rgb(73, 70, 78),
        border_active: Color::Rgb(255, 216, 102),
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
    Style::default().bg(bg())
}

pub fn s_surface() -> Style {
    Style::default().bg(surface())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_selection_and_cycling() {
        assert_eq!(available_themes().len(), 8);

        assert!(set_theme("nord"));
        assert_eq!(current_theme().id, "nord");
        assert_eq!(current_theme().name, "Nord");
        assert_eq!(accent(), current_theme().accent);
        assert_eq!(border_active(), current_theme().border_active);

        assert!(set_theme("dracula"));
        assert_eq!(current_theme().id, "dracula");

        let next = cycle_theme();
        assert_eq!(next, "gruvbox-dark");
        assert_eq!(current_theme().id, "gruvbox-dark");

        // Reset back to default
        assert!(set_theme("tokyo-night"));
        assert_eq!(current_theme().id, "tokyo-night");
    }
}
