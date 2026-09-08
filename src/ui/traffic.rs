use std::collections::VecDeque;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use crate::core_client::protocol::TrafficStats;
use crate::ui::app::format_bytes;
use crate::ui::theme::*;
use ratatui::style::Color;

const TRAFFIC_DOWN: Color = Color::Rgb(74, 222, 128); // Always green (#4ade80)
const TRAFFIC_UP: Color = Color::Rgb(248, 113, 113); // Always red (#f87171)

const DEFAULT_HISTORY_LEN: usize = 60;

#[derive(Debug, Clone, Copy)]
pub struct TrafficPoint {
    pub proxy_down: f64,
    pub proxy_up: f64,
    pub direct_down: f64,
    pub direct_up: f64,
}

pub type ChartPoints = Vec<(f64, f64)>;

#[derive(Debug, Clone)]
pub struct TrafficHistory {
    pub history: VecDeque<TrafficPoint>,
    pub total_proxy_down: i64,
    pub total_proxy_up: i64,
    pub total_direct_down: i64,
    pub total_direct_up: i64,
    pub peak_down: f64,
    pub peak_up: f64,
    pub max_history: usize,
}

impl Default for TrafficHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficHistory {
    pub fn new() -> Self {
        let mut history = VecDeque::with_capacity(DEFAULT_HISTORY_LEN);
        for _ in 0..DEFAULT_HISTORY_LEN {
            history.push_back(TrafficPoint {
                proxy_down: 0.0,
                proxy_up: 0.0,
                direct_down: 0.0,
                direct_up: 0.0,
            });
        }
        Self {
            history,
            total_proxy_down: 0,
            total_proxy_up: 0,
            total_direct_down: 0,
            total_direct_up: 0,
            peak_down: 0.0,
            peak_up: 0.0,
            max_history: DEFAULT_HISTORY_LEN,
        }
    }

    pub fn push(&mut self, stats: &TrafficStats) {
        let down = stats.proxy_down.max(0) as f64;
        let up = stats.proxy_up.max(0) as f64;
        let dir_down = stats.direct_down.max(0) as f64;
        let dir_up = stats.direct_up.max(0) as f64;

        if down > self.peak_down {
            self.peak_down = down;
        }
        if up > self.peak_up {
            self.peak_up = up;
        }

        self.total_proxy_down += stats.proxy_down.max(0);
        self.total_proxy_up += stats.proxy_up.max(0);
        self.total_direct_down += stats.direct_down.max(0);
        self.total_direct_up += stats.direct_up.max(0);

        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(TrafficPoint {
            proxy_down: down,
            proxy_up: up,
            direct_down: dir_down,
            direct_up: dir_up,
        });
    }

    pub fn push_zero(&mut self) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(TrafficPoint {
            proxy_down: 0.0,
            proxy_up: 0.0,
            direct_down: 0.0,
            direct_up: 0.0,
        });
    }

    pub fn chart_points(&self) -> (ChartPoints, ChartPoints) {
        let len = self.history.len();
        let offset = self.max_history.saturating_sub(len);
        let mut down = Vec::with_capacity(len);
        let mut up = Vec::with_capacity(len);
        for (i, p) in self.history.iter().enumerate() {
            let x = (offset + i) as f64;
            down.push((x, p.proxy_down));
            up.push((x, p.proxy_up));
        }
        (down, up)
    }

    pub fn max_y_bound(&self) -> f64 {
        let max_val = self
            .history
            .iter()
            .map(|p| p.proxy_down.max(p.proxy_up))
            .fold(0.0f64, f64::max);

        (max_val * 1.15).max(10.0 * 1024.0)
    }
}

pub fn render_traffic_tab(
    f: &mut Frame,
    area: Rect,
    history: &TrafficHistory,
    is_connected: bool,
    connected_profile: Option<&str>,
    is_tun: bool,
    tun_name: &str,
) {
    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(10),
        Constraint::Length(3),
    ])
    .split(area);

    render_stat_cards(f, chunks[0], history, is_connected, connected_profile);
    render_traffic_chart(f, chunks[1], history);
    render_footer(f, chunks[2], history, is_tun, tun_name);
}

fn render_stat_cards(
    f: &mut Frame,
    area: Rect,
    history: &TrafficHistory,
    is_connected: bool,
    connected_profile: Option<&str>,
) {
    let cards = Layout::horizontal([
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(34),
    ])
    .split(area);

    let current = history.history.back();
    let cur_down = current.map(|c| c.proxy_down as i64).unwrap_or(0);
    let cur_up = current.map(|c| c.proxy_up as i64).unwrap_or(0);

    let down_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border()))
        .title(" Download (RX) ")
        .style(s_bg());
    let down_text = vec![
        Line::from(vec![
            Span::styled(
                "▼ ",
                Style::default()
                    .fg(TRAFFIC_DOWN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s", format_bytes(cur_down)),
                Style::default()
                    .fg(TRAFFIC_DOWN)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Peak: ", s_dim()),
            Span::styled(
                format!("{}/s", format_bytes(history.peak_down as i64)),
                s_text(),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(down_text).block(down_block), cards[0]);

    let up_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border()))
        .title(" Upload (TX) ")
        .style(s_bg());
    let up_text = vec![
        Line::from(vec![
            Span::styled(
                "▲ ",
                Style::default().fg(TRAFFIC_UP).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s", format_bytes(cur_up)),
                Style::default().fg(TRAFFIC_UP).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Peak: ", s_dim()),
            Span::styled(
                format!("{}/s", format_bytes(history.peak_up as i64)),
                s_text(),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(up_text).block(up_block), cards[1]);

    let total_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border()))
        .title(" Session Totals ")
        .style(s_bg());

    let (status_icon, status_style, profile_label) = if is_connected {
        ("●", s_success(), connected_profile.unwrap_or("Connected"))
    } else {
        ("○", s_disconnected(), "Disconnected")
    };

    let total_text = vec![
        Line::from(vec![
            Span::styled("↓ Total: ", s_dim()),
            Span::styled(format_bytes(history.total_proxy_down), s_text()),
            Span::styled("  ↑ Total: ", s_dim()),
            Span::styled(format_bytes(history.total_proxy_up), s_text()),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", status_icon), status_style),
            Span::styled(profile_label, s_text()),
        ]),
    ];
    f.render_widget(Paragraph::new(total_text).block(total_block), cards[2]);
}

fn render_traffic_chart(f: &mut Frame, area: Rect, history: &TrafficHistory) {
    let (down_data, up_data) = history.chart_points();
    let max_y = history.max_y_bound();

    let y_labels = vec![
        Span::styled("0", s_dim()),
        Span::styled(format!("{}/s", format_bytes((max_y * 0.5) as i64)), s_dim()),
        Span::styled(format!("{}/s", format_bytes(max_y as i64)), s_dim()),
    ];

    let x_labels = vec![
        Span::styled("-60s", s_dim()),
        Span::styled("-30s", s_dim()),
        Span::styled("now", s_dim()),
    ];

    let datasets = vec![
        Dataset::default()
            .name("Download (Proxy)")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(TRAFFIC_DOWN))
            .data(&down_data),
        Dataset::default()
            .name("Upload (Proxy)")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(TRAFFIC_UP))
            .data(&up_data),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(" Traffic History (60s) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border()))
                .style(s_bg()),
        )
        .x_axis(
            Axis::default()
                .title(Span::styled("Time", s_dim()))
                .style(Style::default().fg(border()))
                .bounds([0.0, 59.0])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled("Rate", s_dim()))
                .style(Style::default().fg(border()))
                .bounds([0.0, max_y])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

fn render_footer(
    f: &mut Frame,
    area: Rect,
    history: &TrafficHistory,
    is_tun: bool,
    tun_name: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border()))
        .style(s_bg());

    let current = history.history.back();
    let dir_down = current.map(|c| c.direct_down as i64).unwrap_or(0);
    let dir_up = current.map(|c| c.direct_up as i64).unwrap_or(0);

    let (tun_badge, tun_style) = if is_tun {
        (format!("TUN: active ({})", tun_name), s_success())
    } else {
        ("TUN: disabled".to_string(), s_dim())
    };

    let line = Line::from(vec![
        Span::styled(" Direct Bypass: ", s_dim()),
        Span::styled(format!("↓{}/s ", format_bytes(dir_down)), s_text()),
        Span::styled(format!("↑{}/s", format_bytes(dir_up)), s_text()),
        Span::styled("  │  Total Direct: ", s_faint()),
        Span::styled(
            format!("↓{} ", format_bytes(history.total_direct_down)),
            s_dim(),
        ),
        Span::styled(
            format!("↑{}", format_bytes(history.total_direct_up)),
            s_dim(),
        ),
        Span::styled("  │  ", s_faint()),
        Span::styled(tun_badge, tun_style),
    ]);

    f.render_widget(Paragraph::new(line).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traffic_history_initialization() {
        let th = TrafficHistory::new();
        assert_eq!(th.history.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(th.total_proxy_down, 0);
        assert_eq!(th.total_proxy_up, 0);
        assert_eq!(th.peak_down, 0.0);
        assert_eq!(th.peak_up, 0.0);

        let (down, up) = th.chart_points();
        assert_eq!(down.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(up.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(down[0].1, 0.0);
    }

    #[test]
    fn test_traffic_history_push() {
        let mut th = TrafficHistory::new();
        let stats = TrafficStats {
            proxy_down: 1024 * 1024,
            proxy_up: 512 * 1024,
            direct_down: 100,
            direct_up: 50,
        };
        th.push(&stats);

        assert_eq!(th.history.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(th.total_proxy_down, 1024 * 1024);
        assert_eq!(th.total_proxy_up, 512 * 1024);
        assert_eq!(th.total_direct_down, 100);
        assert_eq!(th.total_direct_up, 50);
        assert_eq!(th.peak_down, (1024 * 1024) as f64);
        assert_eq!(th.peak_up, (512 * 1024) as f64);

        let (down, up) = th.chart_points();
        assert_eq!(down.last().unwrap().1, (1024 * 1024) as f64);
        assert_eq!(up.last().unwrap().1, (512 * 1024) as f64);
    }

    #[test]
    fn test_max_y_bound() {
        let mut th = TrafficHistory::new();
        assert_eq!(th.max_y_bound(), 10.0 * 1024.0);

        th.push(&TrafficStats {
            proxy_down: 100 * 1024,
            proxy_up: 0,
            direct_down: 0,
            direct_up: 0,
        });
        assert!((th.max_y_bound() - 115.0 * 1024.0).abs() < 0.01);
    }
}
