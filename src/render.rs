//! Hyprland config generation from neutral profiles.
use crate::model::{GpuPref, Profile, fmt_num};

fn warnings(profile: &Profile, mut w: Vec<String>) -> Vec<String> {
    for m in &profile.monitors {
        if m.output
            .strip_prefix("desc:")
            .is_some_and(|d| d.contains(','))
        {
            w.push(format!(
                "{}: description contains a comma; the conf dialect cannot express it",
                m.output
            ))
        }
    }
    w
}
fn header(profile: &Profile, leader: &str) -> String {
    let mut s = format!(
        "{leader} Profile: {} — generated from {}.toml. Do not edit; edit the TOML.\n{leader}\n",
        profile.name, profile.name
    );
    for m in &profile.matches {
        s.push_str(&format!("{leader}@ match = {m}\n"))
    }
    if profile.edp != crate::model::EdpPolicy::Auto {
        s.push_str(&format!("{leader}@ edp = {}\n", profile.edp.as_str()))
    }
    if profile.gpu != GpuPref::Auto {
        s.push_str(&format!("{leader}@ gpu = {}\n", profile.gpu.as_str()))
    }
    if profile.priority != profile.matches.len() as i64 {
        s.push_str(&format!("{leader}@ priority = {}\n", profile.priority))
    }
    s.push('\n');
    s
}
pub fn render_conf(profile: &Profile) -> (String, Vec<String>) {
    let layout = crate::layout::resolve_all(profile);
    let mut s = header(profile, "#");
    for enabled in [true, false] {
        for o in layout.outputs.iter().filter(|o| o.enabled == enabled) {
            if !o.enabled {
                s.push_str(&format!("monitor = {},disable\n", o.selector));
                continue;
            }
            if let Some(mode) = o.mode {
                s.push_str(&format!(
                    "monitor = {},{},{},{}",
                    o.selector,
                    mode,
                    format_args!("{}x{}", o.position.0, o.position.1),
                    fmt_num(o.scale)
                ));
                if o.transform != 0 {
                    s.push_str(&format!(",transform,{}", o.transform))
                }
                s.push('\n')
            } else {
                s.push_str(&format!(
                    "monitor = {},preferred,auto,{}\n",
                    o.selector,
                    fmt_num(o.scale)
                ))
            }
        }
    }
    if !profile.workspaces.is_empty() {
        s.push('\n')
    }
    for w in &profile.workspaces {
        s.push_str(&format!(
            "workspace = {}, monitor:{}{}\n",
            w.workspace,
            w.monitor,
            if w.default { ", default:true" } else { "" }
        ))
    }
    (s, warnings(profile, layout.warnings))
}
pub fn render_lua(profile: &Profile) -> (String, Vec<String>) {
    let layout = crate::layout::resolve_all(profile);
    let mut s = header(profile, "--");
    for enabled in [true, false] {
        for o in layout.outputs.iter().filter(|o| o.enabled == enabled) {
            if !o.enabled {
                s.push_str(&format!(
                    "hl.monitor({{ output = {:?}, disabled = true }})\n",
                    o.selector
                ));
                continue;
            }
            let mode = o.mode.map_or_else(|| "preferred".into(), |m| m.to_string());
            let pos = if o.mode.is_some() {
                format!("{}x{}", o.position.0, o.position.1)
            } else {
                "auto".into()
            };
            s.push_str(&format!(
                "hl.monitor({{ output = {:?}, mode = {:?}, position = {:?}, scale = {:?}",
                o.selector,
                mode,
                pos,
                fmt_num(o.scale)
            ));
            if o.transform != 0 {
                s.push_str(&format!(", transform = {}", o.transform))
            }
            s.push_str(" })\n")
        }
    }
    if !profile.workspaces.is_empty() {
        s.push('\n')
    }
    for w in &profile.workspaces {
        s.push_str(&format!(
            "hl.workspace_rule({{ workspace = {:?}, monitor = {:?}{} }})\n",
            w.workspace,
            w.monitor,
            if w.default { ", default = true" } else { "" }
        ))
    }
    (s, warnings(profile, layout.warnings))
}

#[cfg(all(test, feature = "hyprland-render"))]
mod tests {
    use super::*;
    use crate::model::*;
    fn dual() -> Profile {
        Profile {
            name: "dual-4k".into(),
            description: String::new(),
            matches: vec![
                "desc:Dell Inc. DELL S2725QC 5DGMS84".into(),
                "desc:Dell Inc. DELL S2725QC FFJMS84".into(),
            ],
            edp: EdpPolicy::Auto,
            gpu: GpuPref::Auto,
            hooks: vec![],
            priority: 2,
            monitors: vec![
                mon("desc:Dell Inc. DELL S2725QC 5DGMS84", "3840x2160@120", 1.5),
                mon("desc:Dell Inc. DELL S2725QC FFJMS84", "3840x2160@120", 1.5),
                mon("eDP-2", "2560x1600@165", 1.5),
            ],
            workspaces: vec![WorkspaceRule {
                workspace: "1".into(),
                monitor: "desc:Dell Inc. DELL S2725QC 5DGMS84".into(),
                default: true,
            }],
        }
    }
    fn mon(o: &str, m: &str, s: f64) -> Monitor {
        Monitor {
            output: o.into(),
            mode: Mode::parse(m),
            scale: s,
            ..Monitor::default()
        }
    }
    #[test]
    fn render_lua_dual_4k_matches_expected() {
        let (s, _) = render_lua(&dual());
        assert!(s.contains("--@ match = desc:Dell Inc. DELL S2725QC 5DGMS84"));
        assert!(s.contains(r#"hl.monitor({ output = "desc:Dell Inc. DELL S2725QC 5DGMS84", mode = "3840x2160@120", position = "0x0", scale = "1.5" })"#));
        assert!(s.contains(r#"hl.monitor({ output = "eDP-2", mode = "2560x1600@165", position = "5120x0", scale = "1.6" })"#));
        assert!(s.contains(r#"hl.workspace_rule({ workspace = "1", monitor = "desc:Dell Inc. DELL S2725QC 5DGMS84", default = true })"#))
    }
    #[test]
    fn render_conf_dual_4k_matches_expected() {
        assert!(
            render_conf(&dual())
                .0
                .contains("monitor = eDP-2,2560x1600@165,5120x0,1.6")
        )
    }
    #[test]
    fn render_conf_transform_and_disabled() {
        let mut p = dual();
        p.monitors[0].transform = 3;
        p.monitors[2].enabled = false;
        let (s, _) = render_conf(&p);
        assert!(s.contains(",transform,3"));
        assert!(s.contains("monitor = eDP-2,disable"))
    }
    #[test]
    fn render_round_trips_through_legacy_parser() {
        let mut p = dual();
        p.edp = EdpPolicy::Disable;
        p.gpu = GpuPref::Dgpu;
        p.priority = 99;
        let (s, _) = render_conf(&p);
        let (q, _) = crate::legacy::to_profile("x", &s).unwrap();
        assert_eq!(
            (q.matches, q.edp, q.gpu, q.priority),
            (p.matches, p.edp, p.gpu, p.priority)
        )
    }
    #[test]
    fn render_warns_on_comma_description() {
        let mut p = dual();
        p.monitors = vec![mon("desc:Weird, Inc. Display", "1920x1080", 1.0)];
        assert_eq!(render_conf(&p).1.len(), 1)
    }
    #[test]
    fn render_omits_default_priority() {
        let p = dual();
        assert!(!render_conf(&p).0.contains("@ priority"));
        let mut p = dual();
        p.priority = 99;
        assert!(render_conf(&p).0.contains("#@ priority = 99"))
    }
}
