use std::collections::VecDeque;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, Paragraph};

use crate::core_client::protocol::TrafficSample;
use crate::ui::app::format_bytes;
use crate::ui::theme::*;

const DEFAULT_HISTORY_LEN: usize = 60;

#[derive(Debug, Clone, Copy)]
pub struct TrafficPoint {
    /// Core sample time (ms). The chart is positioned by time, not by frame
    /// count, so a dropped broadcast shows as a gap instead of stretching the
    /// whole 60-second window.
    pub timestamp_ms: i64,
    pub proxy_down: f64,
    pub proxy_up: f64,
    pub direct_down: f64,
    pub direct_up: f64,
}

const WINDOW_MS: i64 = 60_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn zero_point(timestamp_ms: i64) -> TrafficPoint {
    TrafficPoint {
        timestamp_ms,
        proxy_down: 0.0,
        proxy_up: 0.0,
        direct_down: 0.0,
        direct_up: 0.0,
    }
}

pub type ChartPoints = Vec<(f64, f64)>;

#[derive(Debug, Clone)]
pub struct TrafficHistory {
    pub history: VecDeque<TrafficPoint>,
    /// Core-owned cumulative totals for the current session. Clients never
    /// accumulate deltas: a dropped frame cannot drift them.
    pub total_proxy_down: i64,
    pub total_proxy_up: i64,
    pub total_direct_down: i64,
    pub total_direct_up: i64,
    pub peak_down: f64,
    pub peak_up: f64,
    pub max_history: usize,
    /// Resets the view when the core starts a new connection generation.
    pub session_id: String,
}

impl Default for TrafficHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficHistory {
    pub fn new() -> Self {
        let mut history = VecDeque::with_capacity(DEFAULT_HISTORY_LEN);
        let now = now_ms();
        for i in 0..DEFAULT_HISTORY_LEN {
            history.push_back(zero_point(
                now - (DEFAULT_HISTORY_LEN as i64 - 1 - i as i64) * 1000,
            ));
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
            session_id: String::new(),
        }
    }

    /// Apply a core snapshot. Rates feed the chart; cumulative totals are read
    /// directly from the snapshot, so a dropped broadcast cannot drift them.
    pub fn push(&mut self, stats: &TrafficSample) {
        if !stats.session_id.is_empty() && stats.session_id != self.session_id {
            self.reset(&stats.session_id);
        }

        let down = stats.proxy_rate_down.max(0) as f64;
        let up = stats.proxy_rate_up.max(0) as f64;
        // Both freedom outbounds and kernel-level bypass count as direct.
        let dir_down = (stats.direct_rate_down + stats.bypass_rate_down).max(0) as f64;
        let dir_up = (stats.direct_rate_up + stats.bypass_rate_up).max(0) as f64;

        self.peak_down = self.peak_down.max(down);
        self.peak_up = self.peak_up.max(up);

        self.total_proxy_down = stats.proxy_down.max(0);
        self.total_proxy_up = stats.proxy_up.max(0);
        self.total_direct_down = (stats.direct_down + stats.bypass_down).max(0);
        self.total_direct_up = (stats.direct_up + stats.bypass_up).max(0);

        let timestamp_ms = if stats.timestamp_ms > 0 {
            stats.timestamp_ms
        } else {
            now_ms()
        };
        self.push_point(TrafficPoint {
            timestamp_ms,
            proxy_down: down,
            proxy_up: up,
            direct_down: dir_down,
            direct_up: dir_up,
        });
    }

    fn push_point(&mut self, point: TrafficPoint) {
        self.history.push_back(point);
        // Drop samples outside the 60-second window and cap the buffer so a
        // clock jump cannot grow it without bound. Use the maximum timestamp
        // rather than the tail so out-of-order samples cannot hide the window.
        let newest = self
            .history
            .iter()
            .map(|p| p.timestamp_ms)
            .max()
            .unwrap_or(0);
        while self
            .history
            .front()
            .is_some_and(|p| newest - p.timestamp_ms > WINDOW_MS)
        {
            self.history.pop_front();
        }
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    fn reset(&mut self, session_id: &str) {
        self.history.clear();
        let now = now_ms();
        for i in 0..self.max_history {
            self.history.push_back(zero_point(
                now - (self.max_history as i64 - 1 - i as i64) * 1000,
            ));
        }
        self.total_proxy_down = 0;
        self.total_proxy_up = 0;
        self.total_direct_down = 0;
        self.total_direct_up = 0;
        self.peak_down = 0.0;
        self.peak_up = 0.0;
        self.session_id = session_id.to_string();
    }

    pub fn push_zero(&mut self) {
        self.push_point(zero_point(now_ms()));
    }

    pub fn chart_points(&self) -> (ChartPoints, ChartPoints) {
        let newest = self
            .history
            .iter()
            .map(|p| p.timestamp_ms)
            .max()
            .unwrap_or(0);
        let mut down = Vec::with_capacity(self.history.len());
        let mut up = Vec::with_capacity(self.history.len());
        for p in &self.history {
            // Right edge = now, left edge = 60 seconds ago.
            let age_s = ((newest - p.timestamp_ms).max(0) as f64) / 1000.0;
            let x = (59.0 - age_s).max(0.0);
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
    let show_footer = area.height >= 18;
    let is_compact = area.width < 85;

    let cards_h = if is_compact {
        if area.height < 18 { 3 } else { 5 }
    } else {
        4
    };

    let chunks = if show_footer {
        Layout::vertical([
            Constraint::Length(cards_h),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area)
    } else {
        Layout::vertical([Constraint::Length(cards_h), Constraint::Min(6)]).split(area)
    };

    render_stat_cards(
        f,
        chunks[0],
        history,
        is_connected,
        connected_profile,
        is_compact,
    );
    render_traffic_chart(f, chunks[1], history);
    if show_footer && chunks.len() > 2 {
        render_footer(f, chunks[2], history, is_tun, tun_name);
    }
}

fn render_stat_cards(
    f: &mut Frame,
    area: Rect,
    history: &TrafficHistory,
    is_connected: bool,
    connected_profile: Option<&str>,
    is_compact: bool,
) {
    let current = history.history.back();
    let cur_down = current.map(|c| c.proxy_down as i64).unwrap_or(0);
    let cur_up = current.map(|c| c.proxy_up as i64).unwrap_or(0);

    if is_compact {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border()))
            .title(" Traffic Stats ")
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        let (status_icon, status_style) = if is_connected {
            ("●", s_success())
        } else {
            ("○", s_disconnected())
        };

        if inner.height <= 1 {
            let line = Line::from(vec![
                Span::styled(
                    "▼ ",
                    Style::default()
                        .fg(traffic_down())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}/s ", format_bytes(cur_down)),
                    Style::default().fg(traffic_down()),
                ),
                Span::styled(
                    "▲ ",
                    Style::default()
                        .fg(traffic_up())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}/s ", format_bytes(cur_up)),
                    Style::default().fg(traffic_up()),
                ),
                Span::styled(format!("│ {} ", status_icon), status_style),
                Span::styled(
                    format!(
                        "↓{} ↑{}",
                        format_bytes(history.total_proxy_down),
                        format_bytes(history.total_proxy_up)
                    ),
                    s_dim(),
                ),
            ]);
            f.render_widget(Paragraph::new(line), inner);
            return;
        }

        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

        let r0 = Line::from(vec![
            Span::styled(
                "▼ RX: ",
                Style::default()
                    .fg(traffic_down())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s ", format_bytes(cur_down)),
                Style::default()
                    .fg(traffic_down())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("(Peak: {}/s)", format_bytes(history.peak_down as i64)),
                s_dim(),
            ),
        ]);
        let r1 = Line::from(vec![
            Span::styled(
                "▲ TX: ",
                Style::default()
                    .fg(traffic_up())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s ", format_bytes(cur_up)),
                Style::default()
                    .fg(traffic_up())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("(Peak: {}/s)", format_bytes(history.peak_up as i64)),
                s_dim(),
            ),
        ]);
        let r2 = Line::from(vec![
            Span::styled(format!("{} ", status_icon), status_style),
            Span::styled("Total: ", s_dim()),
            Span::styled(
                format!(
                    "↓{} ↑{}",
                    format_bytes(history.total_proxy_down),
                    format_bytes(history.total_proxy_up)
                ),
                s_text(),
            ),
        ]);
        if !rows.is_empty() {
            f.render_widget(Paragraph::new(r0), rows[0]);
        }
        if rows.len() > 1 {
            f.render_widget(Paragraph::new(r1), rows[1]);
        }
        if rows.len() > 2 {
            f.render_widget(Paragraph::new(r2), rows[2]);
        }
        return;
    }

    let cards = Layout::horizontal([
        Constraint::Percentage(33),
        Constraint::Percentage(33),
        Constraint::Percentage(34),
    ])
    .split(area);

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
                    .fg(traffic_down())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s", format_bytes(cur_down)),
                Style::default()
                    .fg(traffic_down())
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
                Style::default()
                    .fg(traffic_up())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}/s", format_bytes(cur_up)),
                Style::default()
                    .fg(traffic_up())
                    .add_modifier(Modifier::BOLD),
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
            .style(Style::default().fg(traffic_down()))
            .data(&down_data),
        Dataset::default()
            .name("Upload (Proxy)")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(traffic_up()))
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
        assert_eq!(th.session_id, "");

        let (down, up) = th.chart_points();
        assert_eq!(down.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(up.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(down[0].1, 0.0);
    }

    fn sample(session: &str, proxy_down: i64, proxy_up: i64) -> TrafficSample {
        TrafficSample {
            session_id: session.to_string(),
            proxy_down,
            proxy_up,
            proxy_rate_down: proxy_down,
            proxy_rate_up: proxy_up,
            ..TrafficSample::default()
        }
    }

    #[test]
    fn test_traffic_history_push() {
        let mut th = TrafficHistory::new();
        th.push(&sample("1-1000", 1024 * 1024, 512 * 1024));

        assert_eq!(th.history.len(), DEFAULT_HISTORY_LEN);
        assert_eq!(th.total_proxy_down, 1024 * 1024);
        assert_eq!(th.total_proxy_up, 512 * 1024);
        assert_eq!(th.total_direct_down, 0);
        assert_eq!(th.total_direct_up, 0);
        assert_eq!(th.peak_down, (1024 * 1024) as f64);
        assert_eq!(th.peak_up, (512 * 1024) as f64);

        let (down, up) = th.chart_points();
        assert_eq!(down.last().unwrap().1, (1024 * 1024) as f64);
        assert_eq!(up.last().unwrap().1, (512 * 1024) as f64);
    }

    #[test]
    fn test_traffic_history_totals_come_from_snapshot() {
        let mut th = TrafficHistory::new();
        th.push(&sample("1-1000", 1000, 500));
        // A later snapshot carries the session total, not a delta: totals must
        // not grow by summing frames.
        th.push(&TrafficSample {
            session_id: "1-1000".to_string(),
            proxy_down: 4000,
            proxy_up: 900,
            proxy_rate_down: 3000,
            proxy_rate_up: 400,
            ..TrafficSample::default()
        });
        assert_eq!(th.total_proxy_down, 4000);
        assert_eq!(th.total_proxy_up, 900);
    }

    #[test]
    fn test_traffic_history_resets_on_new_session() {
        let mut th = TrafficHistory::new();
        th.push(&sample("1-1000", 1000, 500));
        th.push(&sample("2-2000", 10, 5));
        assert_eq!(th.session_id, "2-2000");
        assert_eq!(th.total_proxy_down, 10);
        assert_eq!(th.peak_down, 10.0);
        assert_eq!(th.history.len(), DEFAULT_HISTORY_LEN);
    }

    #[test]
    fn test_max_y_bound() {
        let mut th = TrafficHistory::new();
        assert_eq!(th.max_y_bound(), 10.0 * 1024.0);

        th.push(&sample("1-1000", 100 * 1024, 0));
        assert!((th.max_y_bound() - 115.0 * 1024.0).abs() < 0.01);
    }

    #[test]
    fn test_chart_is_positioned_by_time() {
        let mut th = TrafficHistory::new();
        th.session_id = "1-1000".to_string();
        th.history.clear();
        for (ts, rate) in [(100_000, 100), (105_000, 200)] {
            th.push(&TrafficSample {
                session_id: "1-1000".to_string(),
                timestamp_ms: ts,
                proxy_rate_down: rate,
                proxy_down: rate,
                ..TrafficSample::default()
            });
        }
        // The initial zero points are outside this synthetic window.
        assert_eq!(th.history.len(), 2);
        let (down, _) = th.chart_points();
        let gap = down.last().unwrap().0 - down[down.len() - 2].0;
        assert!(
            (gap - 5.0).abs() < 0.01,
            "5-second gap must be 5 chart units, got {gap}"
        );
    }
}
