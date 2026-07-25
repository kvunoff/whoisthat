use crate::config;
use crate::core_client::CoreClient;
use crate::ui;

pub(crate) fn build_test_list(app: &ui::App, focused_only: bool) -> Vec<(i32, i32)> {
    let mut list = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut add_profile = |gid: i32, pid: i32| {
        if seen.insert((gid, pid)) {
            list.push((gid, pid));
        }
    };

    if focused_only {
        if app.on_group() {
            if let Some(g) = app.current_group() {
                for p in &g.profiles {
                    add_profile(g.group.id, p.id);
                }
            }
        } else if let Some(p) = app.selected_profile() {
            add_profile(p.group_id, p.id);
        }
        return list;
    }

    if let Some(p) = app.selected_profile() {
        add_profile(p.group_id, p.id);
    } else if let Some(g) = app.current_group() {
        for p in &g.profiles {
            add_profile(g.group.id, p.id);
        }
    }

    for g in &app.groups {
        for p in &g.profiles {
            add_profile(g.group.id, p.id);
        }
    }

    list
}

pub(crate) async fn run_test_batch(app: &mut ui::App, client: &CoreClient, list: &[(i32, i32)]) {
    for (gid, pid) in list {
        app.mark_pending(*gid, *pid);
    }
    let method = app.test_method.clone();
    let samples = app.test_config.samples_per_test;

    let mut single_group = None;
    for (gid, _) in list {
        if *gid != list[0].0 {
            single_group = None;
            break;
        }
        single_group = Some(*gid);
    }
    if let Some(gid) = single_group {
        let _ = client.test_group(gid, &method, samples).await;
    } else {
        for (gid, pid) in list {
            let _ = client.test_profile(*gid, *pid, &method).await;
        }
    }
    app.msg(format!("Testing {} profiles...", list.len()));
}

pub(crate) async fn persist_and_sync_test_config(
    app: &ui::App,
    cfg: &mut config::AppConfig,
    client: &CoreClient,
) {
    cfg.test_concurrency = app.test_config.concurrency;
    cfg.test_timeout_seconds = app.test_config.timeout_seconds;
    cfg.test_samples = app.test_config.samples_per_test;
    cfg.test_endpoint = app.test_config.test_endpoint.clone();
    cfg.auto_test_on_subscribe = app.test_config.auto_test_on_subscribe;
    config::save_config(cfg);
    let _ = client.set_test_config(&app.test_config).await;
}
