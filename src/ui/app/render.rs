use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::helpers::*;
use super::state::App;
use super::types::*;
use crate::ui::logs::render_logs;
use crate::ui::routing::{render_routing_popup, render_routing_tab};
use crate::ui::settings::{SettingsValues, render_settings};
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
            render_routing_popup(
                f,
                popup,
                area,
                self.active_routing_config(),
                self.routing_selected_group(),
            );
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
                Span::styled(" [[]/[]] ", s_accent_bold()),
                Span::styled(format!("{}/5 ", tab_num), s_accent()),
                Span::styled("│ ", s_faint()),
                Span::styled("[?] ", s_accent_bold()),
                Span::styled("│ ", s_faint()),
                Span::styled("[q] ", s_accent_bold()),
            ])
        } else {
            Line::from(vec![
                Span::styled(" [[]/[]] ", s_accent_bold()),
                Span::styled("Tabs ▾ ", s_accent().add_modifier(Modifier::BOLD)),
                Span::styled(format!("({}) ", current_tab_name), s_dim()),
                Span::styled("│ ", s_faint()),
                Span::styled("[?] ", s_accent_bold()),
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
            spans.push(Span::styled(format!("{}/5 ", tab_num), s_accent_bold()));
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
                render_logs(f, area, &mut self.logs_state, focused);
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
                    systemd_available: self.systemd_available,
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
                    kill_switch_armed: self.kill_switch_armed,
                    split_tunnel: &self.split_tunnel,
                    hwid: self.hwid_info.as_ref(),
                    check_updates: self.check_updates,
                };
                render_settings(f, area, &values, &mut self.settings_state, focused);
            }
            ActiveTab::Routing => {
                let focused = self.focus == Focus::LeftPanel;
                render_routing_tab(
                    f,
                    area,
                    self.active_routing_config(),
                    self.routing_cursor,
                    focused,
                    self.routing_selected_group(),
                    self.routing_group_index,
                    self.groups.len() + 1,
                    self.routing_source(),
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

        let xray_label = if let Some(ref x) = self.xray_status {
            if x.status == "downloading" {
                format!(" · xray dl ({}%)", x.progress)
            } else if !x.version.is_empty() {
                format!(" · xray {}", x.version)
            } else if !x.target_version.is_empty() {
                format!(" · xray {}", x.target_version)
            } else {
                " · xray-core".to_string()
            }
        } else {
            " · xray-core".to_string()
        };

        // Live versions: TUI version is compile-time, core version arrives
        // dynamically via `application-state` (never hardcoded in the TUI).
        // Single faint style for the whole left part — no highlighting.
        let tui_version = env!("CARGO_PKG_VERSION");
        let base_str = if is_compact {
            format!(" v{tui_version}")
        } else {
            format!(" WhoisThat v{tui_version}")
        };
        let core_str = if self.core_version.is_empty() {
            " · core …".to_string()
        } else {
            format!(" · core v{}", self.core_version)
        };
        let left_str = if is_compact {
            format!("{base_str}{core_str}")
        } else {
            format!("{base_str}{core_str}{xray_label}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::protocol::{
        Group, GroupWithProfiles, Profile, ProfileDetails, RoutingProfileInfo, TestConfig,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn test_app() -> App {
        App::new(
            false,
            false,
            "warn".to_string(),
            "http-get".to_string(),
            "whoisthattun".to_string(),
            false,
            TestConfig {
                concurrency: 16,
                timeout_seconds: 5,
                samples_per_test: 3,
                test_endpoint: "https://example.com".to_string(),
                auto_test_on_subscribe: false,
            },
        )
    }

    fn render_text(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    fn app_with_profile(uri: &str) -> App {
        let mut app = test_app();
        app.groups = vec![GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Test Group".to_string(),
                ..Default::default()
            },
            profiles: vec![Profile {
                id: 7,
                group_id: 1,
                name: "Vienna".to_string(),
                protocol: "vless".to_string(),
                address: "example.com".to_string(),
                port: 443,
                uri: uri.to_string(),
                ..Default::default()
            }],
            routing_profiles: Vec::new(),
        }];
        app.cursor = 1;
        app
    }

    #[test]
    fn details_show_protocol_specific_fields() {
        let uri = "vless://uuid@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=publickey&sid=1a2b&alpn=h2%2Chttp%2F1.1&flow=xtls-rprx-vision&path=%2Fws#Vienna";
        let mut app = app_with_profile(uri);
        let text = render_text(&mut app, 160, 40);
        assert!(text.contains("Public key"), "details: {text}");
        assert!(text.contains("Short ID"), "details: {text}");
        assert!(text.contains("Fingerprint"), "details: {text}");
        assert!(text.contains("ALPN"), "details: {text}");
    }

    #[test]
    fn details_use_core_metadata_for_managed_profiles() {
        let mut app = app_with_profile("");
        let profile = &mut app.groups[0].profiles[0];
        profile.managed = true;
        profile.protocol = "vless".to_string();
        app.profile_details.insert(
            (1, 7),
            ProfileDetails {
                group_id: 1,
                id: 7,
                transport: "xhttp".to_string(),
                security: "reality".to_string(),
                sni: "sni.example.com".to_string(),
                flow: "xtls-rprx-vision".to_string(),
                alpn: "h2".to_string(),
                path: "/x".to_string(),
                host: "cdn.example.com".to_string(),
                public_key: "pub-key".to_string(),
                short_id: "1a2b".to_string(),
                ..Default::default()
            },
        );
        let text = render_text(&mut app, 160, 40);
        assert!(text.contains("xhttp"), "details: {text}");
        assert!(text.contains("reality"), "details: {text}");
        assert!(text.contains("sni.example.com"), "details: {text}");
        assert!(text.contains("cdn.example.com"), "details: {text}");
        assert!(text.contains("pub-key"), "details: {text}");
    }

    #[test]
    fn details_show_group_routing_context() {
        let mut app = test_app();
        app.groups = vec![GroupWithProfiles {
            group: Group {
                id: 1,
                name: "Test Group".to_string(),
                subscription_url: "https://example.com/sub?token=secret".to_string(),
                routing_mode: "auto".to_string(),
                active_routing_profile_id: "r1".to_string(),
                ..Default::default()
            },
            profiles: Vec::new(),
            routing_profiles: vec![RoutingProfileInfo {
                id: "r1".to_string(),
                name: "Happ Foo".to_string(),
                state: "ok".to_string(),
                rules_count: 3,
                ..Default::default()
            }],
        }];
        let text = render_text(&mut app, 160, 40);
        assert!(text.contains("Routing"), "details: {text}");
        assert!(text.contains("Active"), "details: {text}");
        assert!(text.contains("Happ Foo"), "details: {text}");
        assert!(!text.contains("secret"), "token leaked: {text}");
    }

    #[test]
    fn details_parse_vmess_and_never_show_credentials() {
        use base64::Engine;
        let json = r#"{"add":"example.com","port":"443","id":"secret-id","net":"ws","tls":"tls","path":"/ws","host":"cdn.example.com","sni":"sni.example.com"}"#;
        let payload = base64::engine::general_purpose::STANDARD.encode(json);
        let uri = format!("vmess://{payload}#Vienna");
        let mut app = app_with_profile(&uri);
        let text = render_text(&mut app, 100, 30);
        assert!(text.contains("example.com"), "details: {text}");
        assert!(text.contains("443"), "details: {text}");
        assert!(!text.contains(&payload), "base64 payload leaked: {text}");
        assert!(!text.contains("secret-id"), "credential leaked: {text}");
    }

    #[test]
    fn details_do_not_render_password_secrets() {
        let uri =
            "hysteria2://user:pass@example.com:443?obfs=salamander&obfs-password=topsecret#Vienna";
        let mut app = app_with_profile(uri);
        let text = render_text(&mut app, 160, 40);
        assert!(!text.contains("topsecret"), "secret leaked: {text}");
    }

    #[test]
    fn details_render_in_compact_single_panel() {
        let uri = "vless://uuid@example.com:443?security=tls&sni=example.com#Vienna";
        let mut app = app_with_profile(uri);
        app.focus = Focus::RightPanel;
        let text = render_text(&mut app, 70, 20);
        assert!(text.contains("Profile"), "details: {text}");
        assert!(text.contains("Endpoint"), "details: {text}");
    }

    #[test]
    fn bottom_bar_shows_live_core_version_wide() {
        let mut app = test_app();
        // Deliberately fictional version: proves the value comes from
        // live state (IPC), not from a hardcoded constant in the TUI.
        app.core_version = "9.9.9-test".to_string();
        let text = render_text(&mut app, 120, 30);
        assert!(text.contains("core v9.9.9-test"), "footer: {text}");
    }

    #[test]
    fn bottom_bar_shows_live_core_version_compact() {
        let mut app = test_app();
        app.core_version = "9.9.9-test".to_string();
        let text = render_text(&mut app, 70, 20);
        assert!(text.contains("core v9.9.9-test"), "footer: {text}");
    }

    #[test]
    fn bottom_bar_shows_placeholder_before_handshake() {
        let mut app = test_app();
        assert!(app.core_version.is_empty());
        let text = render_text(&mut app, 120, 30);
        assert!(text.contains("core"), "footer: {text}");
    }
}
