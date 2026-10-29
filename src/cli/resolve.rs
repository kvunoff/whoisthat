use crate::core_client::protocol::{Group, GroupWithProfiles, Profile, RoutingProfileInfo};

use super::error::CliError;

pub fn find_group<'a>(
    groups: &'a [GroupWithProfiles],
    selector: &str,
) -> Result<&'a Group, CliError> {
    if let Ok(id) = selector.parse::<i32>() {
        if let Some(g) = groups.iter().find(|g| g.group.id == id) {
            return Ok(&g.group);
        }
        return Err(CliError::Usage(format!("group #{id} not found")));
    }

    let exact: Vec<&Group> = groups
        .iter()
        .map(|g| &g.group)
        .filter(|g| g.name.eq_ignore_ascii_case(selector))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        return Err(ambiguous(
            selector,
            exact.iter().map(|g| format!("#{} {}", g.id, g.name)),
        ));
    }

    let partial: Vec<&Group> = groups
        .iter()
        .map(|g| &g.group)
        .filter(|g| g.name.to_lowercase().contains(&selector.to_lowercase()))
        .collect();
    match partial.len() {
        1 => Ok(partial[0]),
        0 => Err(CliError::Usage(format!(
            "group '{selector}' not found; run `whoisthat sub list`"
        ))),
        _ => Err(ambiguous(
            selector,
            partial.iter().map(|g| format!("#{} {}", g.id, g.name)),
        )),
    }
}

pub fn find_profile<'a>(
    groups: &'a [GroupWithProfiles],
    selector: &str,
    group_filter: Option<&str>,
) -> Result<(&'a Group, &'a Profile), CliError> {
    // group:profile
    if let Some((g, p)) = selector.split_once(':') {
        if let (Ok(gid), Ok(pid)) = (g.parse::<i32>(), p.parse::<i32>()) {
            if let Some(group) = groups.iter().find(|x| x.group.id == gid) {
                if let Some(profile) = group.profiles.iter().find(|x| x.id == pid) {
                    return Ok((&group.group, profile));
                }
            }
            return Err(CliError::Usage(format!("profile {gid}:{pid} not found")));
        }
    }

    let scoped: Vec<&GroupWithProfiles> = match group_filter {
        Some(sel) => {
            let g = find_group(groups, sel)?;
            groups.iter().filter(|x| x.group.id == g.id).collect()
        }
        None => groups.iter().collect(),
    };

    if let Ok(id) = selector.parse::<i32>() {
        let mut hits = Vec::new();
        for group in &scoped {
            for profile in &group.profiles {
                if profile.id == id {
                    hits.push((&group.group, profile));
                }
            }
        }
        return one_profile(selector, hits);
    }

    let exact: Vec<(&Group, &Profile)> = scoped
        .iter()
        .flat_map(|g| {
            g.profiles
                .iter()
                .filter(|p| p.name.eq_ignore_ascii_case(selector))
                .map(move |p| (&g.group, p))
        })
        .collect();
    if !exact.is_empty() {
        return one_profile(selector, exact);
    }

    let partial: Vec<(&Group, &Profile)> = scoped
        .iter()
        .flat_map(|g| {
            g.profiles
                .iter()
                .filter(|p| p.name.to_lowercase().contains(&selector.to_lowercase()))
                .map(move |p| (&g.group, p))
        })
        .collect();
    match partial.len() {
        0 => Err(CliError::Usage(format!(
            "profile '{selector}' not found; run `whoisthat profile list`"
        ))),
        _ => one_profile(selector, partial),
    }
}

fn one_profile<'a>(
    selector: &str,
    hits: Vec<(&'a Group, &'a Profile)>,
) -> Result<(&'a Group, &'a Profile), CliError> {
    match hits.len() {
        1 => Ok(hits[0]),
        _ => Err(ambiguous(
            selector,
            hits.iter()
                .map(|(g, p)| format!("{}:{} {}", g.id, p.id, p.name)),
        )),
    }
}

pub fn find_routing_profile<'a>(
    group: &'a GroupWithProfiles,
    selector: &str,
) -> Result<&'a RoutingProfileInfo, CliError> {
    if let Some(p) = group.routing_profiles.iter().find(|p| p.id == selector) {
        return Ok(p);
    }
    let exact: Vec<&RoutingProfileInfo> = group
        .routing_profiles
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(selector))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let partial: Vec<&RoutingProfileInfo> = group
        .routing_profiles
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&selector.to_lowercase()))
        .collect();
    match partial.len() {
        1 => Ok(partial[0]),
        0 => Err(CliError::Usage(format!(
            "routing profile '{selector}' not found in group #{}",
            group.group.id
        ))),
        _ => Err(ambiguous(
            selector,
            partial.iter().map(|p| format!("{} {}", p.id, p.name)),
        )),
    }
}

fn ambiguous(selector: &str, items: impl Iterator<Item = String>) -> CliError {
    let list: Vec<String> = items.collect();
    CliError::Usage(format!(
        "ambiguous selector '{selector}': {}",
        list.join(" | ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_client::protocol::Profile;

    fn profile(id: i32, group_id: i32, name: &str) -> Profile {
        Profile {
            id,
            group_id,
            name: name.into(),
            ..Default::default()
        }
    }

    fn groups() -> Vec<GroupWithProfiles> {
        vec![
            GroupWithProfiles {
                group: Group {
                    id: 1,
                    name: "Work".into(),
                    ..Default::default()
                },
                profiles: vec![profile(10, 1, "Berlin"), profile(11, 1, "Berlin-2")],
                routing_profiles: vec![],
            },
            GroupWithProfiles {
                group: Group {
                    id: 2,
                    name: "Home".into(),
                    ..Default::default()
                },
                profiles: vec![profile(10, 2, "Paris")],
                routing_profiles: vec![],
            },
        ]
    }

    #[test]
    fn resolves_group_by_id_name_and_substring() {
        let g = groups();
        assert_eq!(find_group(&g, "1").unwrap().id, 1);
        assert_eq!(find_group(&g, "work").unwrap().id, 1);
        assert_eq!(find_group(&g, "hom").unwrap().id, 2);
        assert!(find_group(&g, "nope").is_err());
    }

    #[test]
    fn resolves_profiles_and_reports_ambiguity() {
        let g = groups();
        assert_eq!(find_profile(&g, "1:11", None).unwrap().1.id, 11);
        assert_eq!(find_profile(&g, "Paris", None).unwrap().1.group_id, 2);
        assert!(find_profile(&g, "Berl", None).is_err(), "must be ambiguous");
        assert!(find_profile(&g, "Berl", Some("Work")).is_err());
        assert_eq!(find_profile(&g, "11", Some("Work")).unwrap().1.id, 11);
    }
}
