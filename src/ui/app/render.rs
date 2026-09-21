use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use super::helpers::*;
use super::state::App;
use super::types::*;
use crate::ui::logs::render_logs;
use crate::ui::routing::{render_routing_popup, render_routing_tab};
use crate::ui::settings::{render_settings, SettingsValues};
use crate::ui::theme::*;
use crate::ui::traffic::render_traffic_tab;

impl App {
    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        self.last_area = area;
        self.layout = crate::ui::layout::LayoutGeometry::compute(area);
        f.render_widget(Block::default().style(s_bg()), area);

        if self.layout.is_too_small {
            let msg = Paragraph::new(format!(
                "Terminal too small ({}x{})\nMinimum size: 45x8\nPlease resize window or press [q] to exit.",
                area.width, area.height
            ))
            .style(s_dim())
            .alignment(Alignment::Center);
            f.render_widget(msg, area);
            return;
        }

        self.render_top_bar(f, self.layout.top_bar);
        self.render_main(f, self.layout.main_area);
        self.render_bottom_bar(f, self.layout.bottom_bar);

        if let Some(ref popup) = self.popup {
            self.render_popup(f, popup, area);
        }
        if let Some(ref popup) = self.routing_popup {
            render_routing_popup(f, popup, area, &self.routing);
        }
    }

    fn render_top_bar(&self, f: &mut Frame, area: Rect) {
        let is_compact = self.layout.width_tier == crate::ui::layout::WidthTier::Compact;
        let is_tiny = self.layout.height_tier == crate::ui::layout::HeightTier::Tiny;
        let is_short = self.layout.height_tier == crate::ui::layout::HeightTier::Short;

        let (icon, icon_style) = if self.is_connected() {
            ("●", s_success())
        } else {
            ("○", s_disconnected())
        };

        let status_text = if self.is_connected() {
            "Connected"
        } else {
            "Disconnected"
        };

        let current_tab_name = match self.tab {
            ActiveTab::Profiles => "Profiles",
            ActiveTab::Routing => "Routing",
            ActiveTab::Traffic => "Traffic",
            ActiveTab::Logs => "Logs",
            ActiveTab::Settings => "Settings",
        };

        let tab_num = match self.tab {
            ActiveTab::Profiles => "1",
            ActiveTab::Routing => "2",
            ActiveTab::Traffic => "3",
            ActiveTab::Logs => "4",
            ActiveTab::Settings => "5",
        };

        let header_actions = if is_compact {
            Line::from(vec![
                Span::styled(" [Tab] ", s_accent_bold()),
                Span::styled(format!("{}/5 ", tab_num), s_accent()),
                Span::styled("│ ", s_faint()),
                Span::styled("[?] ", s_accent_bold()),
                Span::styled("│ ", s_faint()),
                Span::styled("[q] ", s_accent_bold()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" [Tab] ", s_accent_bold()),
                Span::styled("Pages ▾ ", s_accent().add_modifier(Modifier::BOLD)),
                Span::styled(format!("({}) ", current_tab_name), s_dim()),
                Span::styled("│ ", s_faint()),
                Span::styled("[?/h] ", s_accent_bold()),
                Span::styled("Help ", s_text()),
                Span::styled("│ ", s_faint()),
                Span::styled("[Q/q] ", s_accent_bold()),
                Span::styled("Quit/Detach ", s_text()),
            ])
        };

        let ts = &self.traffic_stats;

        if is_tiny {
            // Height 1: 1 line without block border
            let mut spans = vec![
                Span::styled(format!("{} {} ", icon, status_text), icon_style),
                Span::styled("│ ", s_faint()),
                Span::styled(format!("↑{}", format_bytes(ts.proxy_up)), s_success()),
                Span::styled(format!(" ↓{} ", format_bytes(ts.proxy_down)), s_success()),
            ];
            if !is_compact && self.show_ip && !self.public_ip.is_empty() {
                spans.push(Span::styled("│ ", s_faint()));
                spans.push(Span::styled(format!("{} ", self.public_ip), s_dim()));
            }
            spans.push(Span::styled("│ ", s_faint()));
            spans.push(Span::styled(
                format!("[Tab] {}/5 ", tab_num),
                s_accent_bold(),
            ));
            spans.push(Span::styled("│ [?] │ [q]", s_dim()));

            f.render_widget(Paragraph::new(Line::from(spans)).style(s_bg()), area);
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border()))
            .title_alignment(Alignment::Right)
            .title_bottom(header_actions)
            .style(s_bg());

        let inner = block.inner(area);
        f.render_widget(block, area);

        if is_short {
            // Height 3: inner height is 1
            let mut spans = vec![
                Span::styled(" WhoisThat ", s_accent_bold().add_modifier(Modifier::BOLD)),
                Span::styled("│ ", s_faint()),
                Span::styled(format!("{} {} ", icon, status_text), icon_style),
                Span::styled("│ ", s_faint()),
                Span::styled(format!("↑{}", format_bytes(ts.proxy_up)), s_success()),
                Span::styled(format!(" ↓{}", format_bytes(ts.proxy_down)), s_success()),
            ];
            if !is_compact && self.show_ip && !self.public_ip.is_empty() {
                spans.push(Span::styled(" │ ", s_faint()));
                spans.push(Span::styled(self.public_ip.clone(), s_dim()));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), inner);
            return;
        }

        // Height >= 4: inner height is 2
        let mut status_spans = vec![
            Span::styled(" WhoisThat ", s_accent_bold().add_modifier(Modifier::BOLD)),
            Span::styled("│ ", s_faint()),
            Span::styled(format!(" {} {} ", icon, status_text), icon_style),
        ];
        if self.show_ip {
            let ip4 = if self.public_ip.is_empty() {
                "..."
            } else {
                &self.public_ip
            };
            status_spans.push(Span::styled("│ ", s_faint()));
            if is_compact || self.public_ipv6.is_empty() {
                status_spans.push(Span::styled(ip4.to_string(), s_dim()));
            } else {
                status_spans.push(Span::styled(
                    format!("{} {}", ip4, self.public_ipv6),
                    s_dim(),
                ));
            }
        }
        let status_line = Line::from(status_spans);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
        f.render_widget(Paragraph::new(status_line), rows[0]);

        let stats_line = Line::from(vec![
            Span::styled(" P:", s_faint()),
            Span::styled(format!("↑{}", format_bytes(ts.proxy_up)), s_success()),
            Span::styled(format!(" ↓{}", format_bytes(ts.proxy_down)), s_success()),
            Span::styled("  D:", s_faint()),
            Span::styled(format!("↑{}", format_bytes(ts.direct_up)), s_dim()),
            Span::styled(format!(" ↓{}", format_bytes(ts.direct_down)), s_dim()),
        ]);
        f.render_widget(Paragraph::new(stats_line), rows[1]);
    }

    fn render_main(&mut self, f: &mut Frame, area: Rect) {
        match self.tab {
            ActiveTab::Profiles => self.render_profiles_view(f, area),
            ActiveTab::Logs => {
                let focused = self.focus == Focus::LeftPanel;
                render_logs(f, area, &self.logs_state, focused);
            }
            ActiveTab::Settings => {
                let focused = self.focus == Focus::LeftPanel;
                let test_samples = format!("{}", self.test_config.samples_per_test);
                let test_conc = format!("{}", self.test_config.concurrency);
                let test_timeout = format!("{}s", self.test_config.timeout_seconds);
                let endpoint_short = match self.test_config.test_endpoint.as_str() {
                    "https://cp.cloudflare.com/generate_204" => "cloudflare",
                    "https://www.gstatic.com/generate_204" => "gstatic",
                    "https://www.bing.com/" => "bing",
                    other => other,
                }
                .to_string();
                let values = SettingsValues {
                    autoconnect: self.autoconnect_enabled,
                    autostart_mode: &self.autostart_mode,
                    systemd_enabled: self.systemd_enabled,
                    theme: crate::ui::theme::current_theme().name,
                    show_ip: self.show_ip,
                    log_enabled: self.log_enabled,
                    log_level: &self.log_level,
                    test_method: &self.test_method,
                    test_samples: &test_samples,
                    test_concurrency: &test_conc,
                    test_timeout: &test_timeout,
                    test_endpoint: &endpoint_short,
                    auto_test_on_subscribe: self.test_config.auto_test_on_subscribe,
                    tun_name: &self.tun_name,
                    kill_switch_enabled: self.kill_switch_enabled,
                    split_tunnel: &self.split_tunnel,
                    hwid: self.hwid_info.as_ref(),
                };
                render_settings(f, area, &values, &mut self.settings_state, focused);
            }
            ActiveTab::Routing => {
                let focused = self.focus == Focus::LeftPanel;
                render_routing_tab(
                    f,
                    area,
                    &self.routing,
                    self.routing_cursor,
                    focused,
                    self.is_connected_hy2(),
                );
            }
            ActiveTab::Traffic => {
                let connected_name = self.connected_profile_name();
                render_traffic_tab(
                    f,
                    area,
                    &self.traffic_history,
                    self.is_connected(),
                    connected_name,
                    self.tun_enabled,
                    &self.tun_name,
                );
            }
        }
    }

    pub(super) fn render_profiles_view(&mut self, f: &mut Frame, _area: Rect) {
        match self.layout.profiles_mode {
            crate::ui::layout::ProfilesLayoutMode::SideBySide => {
                self.render_tree(f, self.layout.tree_area);
                self.render_details(f, self.layout.details_area);
            }
            crate::ui::layout::ProfilesLayoutMode::Stacked => {
                self.render_tree(f, self.layout.tree_area);
                self.render_details(f, self.layout.details_area);
            }
            crate::ui::layout::ProfilesLayoutMode::SinglePanel => {
                if self.focus == Focus::RightPanel {
                    self.render_details(f, self.layout.tree_area);
                } else {
                    self.render_tree(f, self.layout.tree_area);
                }
            }
        }
    }

    fn render_bottom_bar(&self, f: &mut Frame, area: Rect) {
        let is_compact = self.layout.width_tier == crate::ui::layout::WidthTier::Compact;
        let is_tiny = area.height < 3;

        let inner = if is_tiny {
            area
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border()))
                .style(s_bg());
            let inner = block.inner(area);
            f.render_widget(block, area);
            inner
        };

        let left_str = if is_compact {
            format!(" v{}", env!("CARGO_PKG_VERSION"))
        } else {
            format!(" WhoisThat v{} · xray-core", env!("CARGO_PKG_VERSION"))
        };
        let left = Span::styled(left_str, s_faint());
        let left_w = left.width();

        let tun = if self.tun_enabled {
            Span::styled(" [TUN]", s_success())
        } else {
            Span::styled("", s_dim())
        };
        let tun_w = tun.width();

        let uptime = if self.connection_status.connected_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let elapsed = (now - self.connection_status.connected_at).max(0) as u64;
            let h = elapsed / 3600;
            let m = (elapsed % 3600) / 60;
            let s = elapsed % 60;
            Span::styled(format!(" [{:02}:{:02}:{:02}]", h, m, s), s_success())
        } else {
            Span::styled("", s_dim())
        };
        let uptime_w = uptime.width();

        let inner_w = inner.width as usize;
        let gap = if is_compact { 1 } else { 3 };

        let msg = self.last_msg.as_deref().unwrap_or("");
        let max_right = inner_w.saturating_sub(left_w + tun_w + uptime_w + gap + gap);
        let right = if msg.is_empty() {
            Span::raw("")
        } else if msg.len() <= max_right {
            Span::styled(msg, s_dim())
        } else if max_right > 2 {
            let truncated: String = msg.chars().take(max_right.saturating_sub(2)).collect();
            Span::styled(format!("{}…", truncated), s_dim())
        } else {
            Span::raw("")
        };

        let spans = vec![left, tun, uptime, Span::raw(" ".repeat(gap)), right];
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
    }
}
