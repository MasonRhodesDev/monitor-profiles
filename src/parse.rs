//! TOML profile parsing. Every failure is contained.
use crate::model::{EdpPolicy, GpuPref, Mode, Monitor, Profile, WorkspaceRule};
use serde::Deserialize;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub source: String,
    pub message: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    #[serde(default)]
    description: String,
    #[serde(default, rename = "match")]
    matches: Vec<String>,
    #[serde(default)]
    edp: Option<String>,
    #[serde(default)]
    gpu: Option<String>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    monitor: Vec<RawMonitor>,
    #[serde(default)]
    workspace: Vec<RawWorkspace>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMonitor {
    output: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    position: Option<[i32; 2]>,
    #[serde(default)]
    transform: Option<u8>,
    #[serde(default)]
    enabled: Option<bool>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    workspace: String,
    monitor: String,
    #[serde(default)]
    default: bool,
}
pub fn from_toml(name: &str, text: &str) -> Result<(Profile, Vec<String>), String> {
    let raw: RawProfile = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut warnings = vec![];
    if raw.matches.is_empty() {
        return Err("profile has no `match` entries".into());
    }
    let edp = match raw.edp.as_deref() {
        None => EdpPolicy::default(),
        Some(v) => EdpPolicy::parse(v)
            .ok_or_else(|| format!("edp must be auto|enable|disable, got {v:?}"))?,
    };
    let gpu = match raw.gpu.as_deref() {
        None => GpuPref::default(),
        Some(v) => {
            GpuPref::parse(v).ok_or_else(|| format!("gpu must be auto|igpu|dgpu, got {v:?}"))?
        }
    };
    let mut monitors = vec![];
    for m in raw.monitor {
        if m.output.trim().is_empty() {
            warnings.push("skipping monitor with empty output".into());
            continue;
        }
        let mode = match m.mode.as_deref() {
            None => None,
            Some(s) => match Mode::parse(s) {
                Some(x) => Some(x),
                None => {
                    warnings.push(format!(
                        "{}: unparseable mode {s:?}; using preferred",
                        m.output
                    ));
                    None
                }
            },
        };
        let scale = match m.scale {
            None => 1.0,
            Some(s) if s.is_finite() && s > 0.0 => s,
            Some(s) => {
                warnings.push(format!("{}: invalid scale {s}; using 1.0", m.output));
                1.0
            }
        };
        let transform = match m.transform {
            None => 0,
            Some(t) if t <= 7 => t,
            Some(t) => {
                warnings.push(format!(
                    "{}: transform {t} out of range 0..=7; using 0",
                    m.output
                ));
                0
            }
        };
        monitors.push(Monitor {
            output: m.output,
            mode,
            scale,
            position: m.position.map(|p| (p[0], p[1])),
            transform,
            enabled: m.enabled.unwrap_or(true),
        });
    }
    let workspaces = raw
        .workspace
        .into_iter()
        .map(|w| WorkspaceRule {
            workspace: w.workspace,
            monitor: w.monitor,
            default: w.default,
        })
        .collect();
    let priority = raw.priority.unwrap_or(raw.matches.len() as i64);
    Ok((
        Profile {
            name: name.into(),
            description: raw.description,
            matches: raw.matches,
            edp,
            gpu,
            hooks: raw.hooks,
            priority,
            monitors,
            workspaces,
        },
        warnings,
    ))
}
pub fn load_dir(dir: &Path) -> (Vec<Profile>, Vec<Diagnostic>) {
    let (mut ps, mut ds) = (vec![], vec![]);
    let entries = match std::fs::read_dir(dir) {
        Ok(x) => x,
        // An absent directory is not a fault: it is what "no profiles
        // configured" looks like on a machine where nothing ships them. Only
        // a directory that exists but cannot be read is worth a diagnostic --
        // consumers poll this path on every run and would otherwise warn
        // forever about a file the user never asked for.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (ps, ds),
        Err(e) => {
            ds.push(Diagnostic {
                source: dir.display().to_string(),
                message: format!("cannot read profile directory: {e}"),
            });
            return (ps, ds);
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    paths.sort();
    for path in paths {
        let source = path.display().to_string();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(x) => x,
            Err(e) => {
                ds.push(Diagnostic {
                    source,
                    message: format!("unreadable: {e}"),
                });
                continue;
            }
        };
        match from_toml(&name, &text) {
            Ok((p, ws)) => {
                for message in ws {
                    ds.push(Diagnostic {
                        source: source.clone(),
                        message,
                    })
                }
                ps.push(p)
            }
            Err(message) => ds.push(Diagnostic {
                source,
                message: format!("skipped: {message}"),
            }),
        }
    }
    (ps, ds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    const FULL: &str = r#"description="d"
match=["A","B"]
edp="disable"
gpu="dgpu"
hooks=["h"]
priority=10
[[monitor]]
output="A"
mode="3840x2160@120"
scale=1.5
position=[3440,0]
transform=3
[[monitor]]
output="B"
[[workspace]]
workspace="1"
monitor="A"
default=true
"#;
    #[test]
    fn parses_full_profile() {
        let (p, w) = from_toml("n", FULL).unwrap();
        assert!(w.is_empty());
        assert_eq!(p.name, "n");
        assert_eq!(p.description, "d");
        assert_eq!(p.matches.len(), 2);
        assert_eq!(p.edp, EdpPolicy::Disable);
        assert_eq!(p.gpu, GpuPref::Dgpu);
        assert_eq!(p.hooks, vec!["h"]);
        assert_eq!(p.priority, 10);
        assert_eq!(p.monitors[0].position, Some((3440, 0)));
        assert_eq!(p.monitors[0].transform, 3);
        assert_eq!(p.monitors[1].scale, 1.0);
        assert!(p.monitors[1].enabled);
        assert!(p.workspaces[0].default);
    }
    #[test]
    fn default_priority_is_match_count() {
        assert_eq!(from_toml("n", "match=[\"A\",\"B\"]").unwrap().0.priority, 2)
    }
    #[test]
    fn no_matches_is_fatal() {
        assert!(from_toml("n", "[[monitor]]\noutput=\"A\"").is_err())
    }
    #[test]
    fn bad_edp_and_gpu_are_fatal() {
        assert!(from_toml("n", "match=[\"A\"]\nedp=\"sideways\"").is_err());
        assert!(from_toml("n", "match=[\"A\"]\ngpu=\"both\"").is_err())
    }
    #[test]
    fn bad_scale_mode_transform_warn_but_load() {
        let (p, w) = from_toml(
            "n",
            "match=[\"A\"]\n[[monitor]]\noutput=\"A\"\nscale=-1\nmode=\"nonsense\"\ntransform=9",
        )
        .unwrap();
        assert_eq!(w.len(), 3);
        assert_eq!(p.monitors[0].scale, 1.0);
        assert_eq!(p.monitors[0].mode, None);
        assert_eq!(p.monitors[0].transform, 0)
    }
    #[test]
    fn unknown_key_is_fatal() {
        assert!(from_toml("n", "match=[\"A\"]\nbogus=1").is_err())
    }
    #[test]
    fn absent_directory_is_silent_not_a_diagnostic() {
        let missing = std::path::Path::new("/nonexistent/monitor-profiles-absent");
        let (profiles, diagnostics) = load_dir(missing);
        assert!(profiles.is_empty());
        assert!(
            diagnostics.is_empty(),
            "a missing dir must not warn on every run: {diagnostics:?}"
        );
    }

    #[test]
    fn load_dir_keeps_good_skips_bad() {
        let d = std::env::temp_dir().join(format!("monitor-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir(&d).unwrap();
        std::fs::write(d.join("good.toml"), "match=[\"A\"]").unwrap();
        std::fs::write(d.join("bad.toml"), "description=\"x\"").unwrap();
        std::fs::write(d.join(".hidden.toml"), "match=[\"H\"]").unwrap();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        let (p, x) = load_dir(&d);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "good");
        assert!(x.iter().any(|d| d.source.contains("bad.toml")));
        assert!(!x.iter().any(|d| d.source.contains("hidden")));
        std::fs::remove_dir_all(d).unwrap()
    }
    #[test]
    /// A directory that exists but cannot be read is a real fault worth
    /// reporting -- unlike one that simply is not there, which is the
    /// ordinary state on a machine that ships no profiles.
    fn load_dir_unreadable_directory_is_a_diagnostic() {
        let d = std::env::temp_dir().join(format!(
            "monitor-profiles-unreadable-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        // Strip every permission bit so read_dir fails with something other
        // than NotFound. Skipped when running as root, which ignores them.
        std::fs::set_permissions(&d, PermissionsExt::from_mode(0o000)).unwrap();
        let (p, x) = load_dir(&d);
        let running_as_root = x.is_empty();
        let _ = std::fs::set_permissions(&d, PermissionsExt::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&d);
        assert!(p.is_empty());
        if !running_as_root {
            assert_eq!(x.len(), 1);
        }
    }
}
