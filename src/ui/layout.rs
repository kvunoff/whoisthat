use ratatui::layout::{Constraint, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WidthTier {
    /// < 80 cols: Narrow / tmux splits / mobile
    Compact,
    /// 80..=125 cols: Standard terminal windows
    #[default]
    Medium,
    /// 126..=160 cols: Wide terminal windows
    Wide,
    /// > 160 cols: Ultrawide / full-screen FHD/2K/4K
    UltraWide,
}

impl WidthTier {
    pub fn from_width(width: u16) -> Self {
        match width {
            w if w < 80 => Self::Compact,
            w if w <= 125 => Self::Medium,
            w if w <= 160 => Self::Wide,
            _ => Self::UltraWide,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeightTier {
    /// < 16 rows: Tiny / critical
    Tiny,
    /// 16..=24 rows: Short
    Short,
    /// 25..=42 rows: Standard
    #[default]
    Normal,
    /// > 42 rows: Tall
    Tall,
}

impl HeightTier {
    pub fn from_height(height: u16) -> Self {
        match height {
            h if h < 16 => Self::Tiny,
            h if h <= 24 => Self::Short,
            h if h <= 42 => Self::Normal,
            _ => Self::Tall,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfilesLayoutMode {
    /// Side by side (horizontal split)
    #[default]
    SideBySide,
    /// Stacked (vertical split: Tree on top, Details below)
    Stacked,
    /// Single panel (Tree takes 100% of space)
    SinglePanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrafficLayoutMode {
    /// 3 cards in a horizontal row
    #[default]
    Row3,
    /// 3 cards stacked vertically
    Stack3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutGeometry {
    pub area: Rect,
    pub width_tier: WidthTier,
    pub height_tier: HeightTier,
    pub is_too_small: bool,

    pub top_bar: Rect,
    pub main_area: Rect,
    pub bottom_bar: Rect,

    pub profiles_mode: ProfilesLayoutMode,
    pub tree_area: Rect,
    pub details_area: Rect,

    pub traffic_cards_area: Rect,
    pub traffic_cards: [Rect; 3],
    pub traffic_chart_area: Rect,
    pub traffic_footer_area: Rect,
    pub traffic_layout_mode: TrafficLayoutMode,
    pub show_traffic_footer: bool,
}

impl Default for LayoutGeometry {
    fn default() -> Self {
        Self {
            area: Rect::default(),
            width_tier: WidthTier::default(),
            height_tier: HeightTier::default(),
            is_too_small: false,
            top_bar: Rect::default(),
            main_area: Rect::default(),
            bottom_bar: Rect::default(),
            profiles_mode: ProfilesLayoutMode::SideBySide,
            tree_area: Rect::default(),
            details_area: Rect::default(),
            traffic_cards_area: Rect::default(),
            traffic_cards: [Rect::default(); 3],
            traffic_chart_area: Rect::default(),
            traffic_footer_area: Rect::default(),
            traffic_layout_mode: TrafficLayoutMode::Row3,
            show_traffic_footer: true,
        }
    }
}

impl LayoutGeometry {
    pub fn compute(area: Rect) -> Self {
        let is_too_small = area.width < 45 || area.height < 8;

        let width_tier = WidthTier::from_width(area.width);
        let height_tier = HeightTier::from_height(area.height);

        let (top_h, bottom_h) = match height_tier {
            HeightTier::Tiny => (1, 1),
            HeightTier::Short => (3, 2),
            HeightTier::Normal | HeightTier::Tall => (4, 3),
        };

        let v_chunks = Layout::vertical([
            Constraint::Length(top_h),
            Constraint::Min(0),
            Constraint::Length(bottom_h),
        ])
        .split(area);

        let top_bar = v_chunks[0];
        let main_area = v_chunks[1];
        let bottom_bar = v_chunks[2];

        // Profiles view layout
        let (profiles_mode, tree_area, details_area) = match width_tier {
            WidthTier::Compact => {
                if main_area.height >= 18 {
                    let splits =
                        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                            .split(main_area);
                    (ProfilesLayoutMode::Stacked, splits[0], splits[1])
                } else {
                    (ProfilesLayoutMode::SinglePanel, main_area, Rect::default())
                }
            }
            WidthTier::Medium => {
                let splits =
                    Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                        .split(main_area);
                (ProfilesLayoutMode::SideBySide, splits[0], splits[1])
            }
            WidthTier::Wide | WidthTier::UltraWide => {
                let splits =
                    Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                        .split(main_area);
                (ProfilesLayoutMode::SideBySide, splits[0], splits[1])
            }
        };

        // Traffic tab layout
        let show_traffic_footer = height_tier != HeightTier::Tiny && main_area.height >= 14;
        let traffic_layout_mode = if width_tier == WidthTier::Compact {
            TrafficLayoutMode::Stack3
        } else {
            TrafficLayoutMode::Row3
        };

        let traffic_cards_h = if traffic_layout_mode == TrafficLayoutMode::Stack3 {
            // 3 lines or compact stack
            if main_area.height < 18 {
                3
            } else {
                6
            }
        } else {
            4
        };

        let traffic_chunks = if show_traffic_footer {
            Layout::vertical([
                Constraint::Length(traffic_cards_h),
                Constraint::Min(6),
                Constraint::Length(3),
            ])
            .split(main_area)
        } else {
            Layout::vertical([Constraint::Length(traffic_cards_h), Constraint::Min(4)])
                .split(main_area)
        };

        let traffic_cards_area = traffic_chunks[0];
        let traffic_chart_area = traffic_chunks[1];
        let traffic_footer_area = if show_traffic_footer && traffic_chunks.len() > 2 {
            traffic_chunks[2]
        } else {
            Rect::default()
        };

        let traffic_cards = match traffic_layout_mode {
            TrafficLayoutMode::Row3 => {
                let splits = Layout::horizontal([
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                ])
                .split(traffic_cards_area);
                [splits[0], splits[1], splits[2]]
            }
            TrafficLayoutMode::Stack3 => {
                let splits = Layout::vertical([
                    Constraint::Ratio(1, 3),
                    Constraint::Ratio(1, 3),
                    Constraint::Ratio(1, 3),
                ])
                .split(traffic_cards_area);
                [splits[0], splits[1], splits[2]]
            }
        };

        Self {
            area,
            width_tier,
            height_tier,
            is_too_small,
            top_bar,
            main_area,
            bottom_bar,
            profiles_mode,
            tree_area,
            details_area,
            traffic_cards_area,
            traffic_cards,
            traffic_chart_area,
            traffic_footer_area,
            traffic_layout_mode,
            show_traffic_footer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_classification() {
        let tiny_compact = LayoutGeometry::compute(Rect::new(0, 0, 70, 14));
        assert_eq!(tiny_compact.width_tier, WidthTier::Compact);
        assert_eq!(tiny_compact.height_tier, HeightTier::Tiny);
        assert_eq!(tiny_compact.top_bar.height, 1);
        assert_eq!(tiny_compact.bottom_bar.height, 1);

        let standard = LayoutGeometry::compute(Rect::new(0, 0, 100, 30));
        assert_eq!(standard.width_tier, WidthTier::Medium);
        assert_eq!(standard.height_tier, HeightTier::Normal);
        assert_eq!(standard.top_bar.height, 4);
        assert_eq!(standard.bottom_bar.height, 3);
        assert_eq!(standard.profiles_mode, ProfilesLayoutMode::SideBySide);

        let ultrawide_tall = LayoutGeometry::compute(Rect::new(0, 0, 200, 60));
        assert_eq!(ultrawide_tall.width_tier, WidthTier::UltraWide);
        assert_eq!(ultrawide_tall.height_tier, HeightTier::Tall);
        assert_eq!(ultrawide_tall.profiles_mode, ProfilesLayoutMode::SideBySide);
    }

    #[test]
    fn test_compact_profiles_stacked_vs_single() {
        let compact_tall = LayoutGeometry::compute(Rect::new(0, 0, 70, 30));
        assert_eq!(compact_tall.width_tier, WidthTier::Compact);
        assert_eq!(compact_tall.profiles_mode, ProfilesLayoutMode::Stacked);
        assert!(compact_tall.details_area.height > 0);

        let compact_short = LayoutGeometry::compute(Rect::new(0, 0, 70, 15));
        assert_eq!(compact_short.width_tier, WidthTier::Compact);
        assert_eq!(compact_short.profiles_mode, ProfilesLayoutMode::SinglePanel);
        assert_eq!(compact_short.details_area.height, 0);
    }

    #[test]
    fn test_traffic_cards_responsive() {
        let compact = LayoutGeometry::compute(Rect::new(0, 0, 75, 25));
        assert_eq!(compact.traffic_layout_mode, TrafficLayoutMode::Stack3);

        let standard = LayoutGeometry::compute(Rect::new(0, 0, 110, 25));
        assert_eq!(standard.traffic_layout_mode, TrafficLayoutMode::Row3);
    }

    #[test]
    fn test_safety_threshold() {
        let tiny = LayoutGeometry::compute(Rect::new(0, 0, 40, 7));
        assert!(tiny.is_too_small);

        let ok = LayoutGeometry::compute(Rect::new(0, 0, 50, 10));
        assert!(!ok.is_too_small);
    }

    #[test]
    fn test_boundary_width_tiers() {
        assert_eq!(WidthTier::from_width(79), WidthTier::Compact);
        assert_eq!(WidthTier::from_width(80), WidthTier::Medium);
        assert_eq!(WidthTier::from_width(125), WidthTier::Medium);
        assert_eq!(WidthTier::from_width(126), WidthTier::Wide);
        assert_eq!(WidthTier::from_width(160), WidthTier::Wide);
        assert_eq!(WidthTier::from_width(161), WidthTier::UltraWide);
    }

    #[test]
    fn test_boundary_height_tiers() {
        assert_eq!(HeightTier::from_height(15), HeightTier::Tiny);
        assert_eq!(HeightTier::from_height(16), HeightTier::Short);
        assert_eq!(HeightTier::from_height(24), HeightTier::Short);
        assert_eq!(HeightTier::from_height(25), HeightTier::Normal);
        assert_eq!(HeightTier::from_height(42), HeightTier::Normal);
        assert_eq!(HeightTier::from_height(43), HeightTier::Tall);
    }

    #[test]
    fn test_traffic_footer_visibility() {
        let tiny = LayoutGeometry::compute(Rect::new(0, 0, 100, 15));
        assert!(!tiny.show_traffic_footer);

        let normal = LayoutGeometry::compute(Rect::new(0, 0, 100, 25));
        assert!(normal.show_traffic_footer);
    }
}
