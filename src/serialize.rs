//! Serialize a [`Profile`] back to the neutral TOML dialect.
//!
//! TOML is the only hand-edited source of truth. Session managers and tools
//! must write through this path rather than emitting compositor dialects.

use crate::model::{EdpPolicy, GpuPref, Profile, fmt_num};

fn quote(value: &str) -> String {
    // toml string escaping for our subset (no multiline values).
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Emit the canonical TOML form of `profile` (without a leading name —
/// the stem of the file is the profile name).
pub fn to_toml(profile: &Profile) -> String {
    let mut out = String::new();
    if !profile.description.is_empty() {
        out.push_str(&format!("description = {}\n", quote(&profile.description)));
    }
    out.push_str("match = [");
    out.push_str(
        &profile
            .matches
            .iter()
            .map(|x| quote(x))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("]\n");
    if profile.edp != EdpPolicy::Auto {
        out.push_str(&format!("edp = {}\n", quote(profile.edp.as_str())));
    }
    if profile.gpu != GpuPref::Auto {
        out.push_str(&format!("gpu = {}\n", quote(profile.gpu.as_str())));
    }
    if !profile.hooks.is_empty() {
        out.push_str("hooks = [");
        out.push_str(
            &profile
                .hooks
                .iter()
                .map(|x| quote(x))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("]\n");
    }
    if profile.priority != profile.matches.len() as i64 {
        out.push_str(&format!("priority = {}\n", profile.priority));
    }
    for monitor in &profile.monitors {
        out.push_str("\n[[monitor]]\n");
        out.push_str(&format!("output = {}\n", quote(&monitor.output)));
        if let Some(mode) = monitor.mode {
            out.push_str(&format!("mode = {}\n", quote(&mode.to_string())));
        }
        if (monitor.scale - 1.0).abs() > f64::EPSILON {
            out.push_str(&format!("scale = {}\n", fmt_num(monitor.scale)));
        }
        if let Some((x, y)) = monitor.position {
            out.push_str(&format!("position = [{x}, {y}]\n"));
        }
        if monitor.transform != 0 {
            out.push_str(&format!("transform = {}\n", monitor.transform));
        }
        if !monitor.enabled {
            out.push_str("enabled = false\n");
        }
    }
    for workspace in &profile.workspaces {
        out.push_str("\n[[workspace]]\n");
        out.push_str(&format!("workspace = {}\n", quote(&workspace.workspace)));
        out.push_str(&format!("monitor = {}\n", quote(&workspace.monitor)));
        if workspace.default {
            out.push_str("default = true\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mode, Monitor, WorkspaceRule};

    #[test]
    fn round_trip_portrait_desk_profile() {
        let profile = Profile {
            name: "ultrawide-with-secondary".into(),
            description: "ultrawide + portrait".into(),
            matches: vec![
                "desc:Dell Inc. DELL S3422DWG".into(),
                "desc:Dell Inc. DELL S2721QS".into(),
            ],
            edp: EdpPolicy::Auto,
            gpu: GpuPref::Auto,
            hooks: vec![],
            priority: 2,
            monitors: vec![
                Monitor {
                    output: "desc:Dell Inc. DELL S3422DWG".into(),
                    mode: Mode::parse("3440x1440@144"),
                    ..Monitor::default()
                },
                Monitor {
                    output: "desc:Dell Inc. DELL S2721QS".into(),
                    mode: Mode::parse("3840x2160@60"),
                    scale: 1.5,
                    transform: 3,
                    ..Monitor::default()
                },
            ],
            workspaces: vec![WorkspaceRule {
                workspace: "1".into(),
                monitor: "desc:Dell Inc. DELL S3422DWG".into(),
                default: true,
            }],
        };
        let text = to_toml(&profile);
        let (parsed, warnings) =
            crate::parse::from_toml("ultrawide-with-secondary", &text).expect("parse");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(parsed.matches, profile.matches);
        assert_eq!(parsed.monitors.len(), 2);
        assert_eq!(parsed.monitors[1].transform, 3);
        assert!((parsed.monitors[1].scale - 1.5).abs() < f64::EPSILON);
        assert_eq!(parsed.workspaces.len(), 1);
        assert!(parsed.workspaces[0].default);
    }
}
