//! Best-effort migration from legacy Hyprland `.conf`/`.lua` profiles.
//!
//! **Deprecated as a profile source.** Hand-edited dialect files are no
//! longer the source of truth — convert once with [`to_profile`] (or the
//! `monitor-profiles migrate` CLI) into TOML, then edit the TOML. This
//! module remains for one-shot migration only.
use crate::model::{EdpPolicy, GpuPref, Mode, Monitor, Profile, WorkspaceRule};
pub fn parse_directive(line: &str, allow_hyphen: bool) -> Option<(&str, &str)> {
    let rest = line
        .strip_prefix("#@")
        .or_else(|| line.strip_prefix("--@"))?
        .trim_start();
    let eq = rest.find('=')?;
    let key = rest[..eq].trim_end();
    let val = rest[eq + 1..].trim();
    if key.is_empty() || val.is_empty() {
        return None;
    }
    let ok = key
        .chars()
        .enumerate()
        .all(|(i, c)| c.is_ascii_lowercase() || (allow_hyphen && i > 0 && c == '-'));
    ok.then_some((key, val))
}
pub fn to_profile(name: &str, text: &str) -> Result<(Profile, Vec<String>), String> {
    let mut matches = vec![];
    let mut hooks = vec![];
    let (mut edp, mut gpu, mut priority) = (EdpPolicy::Auto, GpuPref::Auto, None);
    let mut warnings = vec![];
    let mut description = String::new();
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(x) = t
            .strip_prefix("# Profile: ")
            .or_else(|| t.strip_prefix("-- Profile: "))
        {
            description = x.trim().into()
        }
        if t.starts_with("#@") || t.starts_with("--@") {
            match parse_directive(t, false) {
                Some(("match", v)) => matches.push(v.into()),
                Some(("hook", v)) => hooks.push(v.into()),
                Some(("edp", v)) => {
                    edp = EdpPolicy::parse(v).ok_or_else(|| format!("invalid edp {v:?}"))?
                }
                Some(("gpu", v)) => {
                    gpu = GpuPref::parse(v).ok_or_else(|| format!("invalid gpu {v:?}"))?
                }
                Some(("priority", v)) => {
                    priority = Some(v.parse().map_err(|_| format!("invalid priority {v:?}"))?)
                }
                Some((k, _)) => warnings.push(format!("unknown directive {k:?}")),
                None => warnings.push(format!("malformed directive {t:?}")),
            }
            continue;
        }
        if t.is_empty() || t.starts_with('#') || t.starts_with("--") {
            continue;
        }
        break;
    }
    if matches.is_empty() {
        return Err("profile has no `match` entries".into());
    }
    let (mut monitors, mut body_warnings) = parse_conf_monitors(text);
    if monitors.is_empty() {
        (monitors, body_warnings) = parse_mon_row(text);
        if monitors.is_empty() {
            // The other Lua dialect: explicit per-monitor calls, used when a
            // row cannot express the arrangement.
            let (hl, mut hl_warnings) = parse_hl_monitor(text);
            if !hl.is_empty() {
                monitors = hl;
                body_warnings.append(&mut hl_warnings);
            }
        }
    }
    warnings.append(&mut body_warnings);
    if monitors.is_empty() {
        warnings.push("body has no recognisable monitor entries; convert by hand".into())
    }
    let priority = priority.unwrap_or(matches.len() as i64);
    Ok((
        Profile {
            name: name.into(),
            description,
            matches,
            edp,
            gpu,
            hooks,
            priority,
            monitors,
            workspaces: parse_workspace_rules(text),
        },
        warnings,
    ))
}
pub fn parse_conf_monitors(text: &str) -> (Vec<Monitor>, Vec<String>) {
    let mut out = vec![];
    let mut warnings = vec![];
    for line in text.lines() {
        let t = line.trim();
        let Some(rhs) = t
            .strip_prefix("monitor")
            .and_then(|x| x.trim_start().strip_prefix('='))
            .map(str::trim)
        else {
            continue;
        };
        let p: Vec<_> = rhs.split(',').map(str::trim).collect();
        if p.len() >= 2 && p[1] == "disable" {
            out.push(Monitor {
                output: p[0].into(),
                enabled: false,
                ..Monitor::default()
            });
            continue;
        }
        if p.len() < 4 {
            warnings.push(format!("unparseable monitor line {t:?}"));
            continue;
        }
        // Hyprland accepts keywords where a value is expected: `preferred`
        // / `highres` / `highrr` for the mode, `auto*` for position, and
        // `auto` for scale. They mean "let the compositor decide", which is
        // exactly what None means here — rejecting the line instead would
        // silently drop the whole monitor (it did: two real profiles
        // migrated to zero monitors before this was handled).
        let mode = match p[1] {
            "preferred" | "highres" | "highrr" => None,
            other => match Mode::parse(other) {
                Some(mode) => Some(mode),
                None => {
                    warnings.push(format!("unparseable monitor mode {other:?}"));
                    continue;
                }
            },
        };
        let position = if p[2].starts_with("auto") {
            None
        } else {
            let Some((xs, ys)) = p[2].split_once('x') else {
                warnings.push(format!("unparseable monitor position {:?}", p[2]));
                continue;
            };
            let (Ok(x), Ok(y)) = (xs.parse::<i32>(), ys.parse::<i32>()) else {
                warnings.push(format!("unparseable monitor position {:?}", p[2]));
                continue;
            };
            Some((x, y))
        };
        let scale = if p[3] == "auto" {
            1.0
        } else {
            match p[3].parse() {
                Ok(scale) => scale,
                Err(_) => {
                    warnings.push(format!("unparseable monitor scale {:?}", p[3]));
                    continue;
                }
            }
        };
        let mut transform = 0;
        if p.len() >= 6 && p[4] == "transform" {
            transform = p[5].parse().unwrap_or_else(|_| {
                warnings.push(format!("unparseable transform {:?}", p[5]));
                0
            })
        }
        out.push(Monitor {
            output: p[0].into(),
            mode,
            scale,
            position,
            transform,
            enabled: true,
        })
    }
    (out, warnings)
}
fn field<'a>(group: &'a str, key: &str) -> Option<&'a str> {
    let pos = group.match_indices(key).find_map(|(pos, _)| {
        let before = group[..pos].chars().next_back();
        let after = group[pos + key.len()..].chars().next();
        (!before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then_some(pos)
    })?;
    let mut s = group[pos + key.len()..].trim_start();
    s = s.strip_prefix('=')?.trim_start();
    Some(s.split([',', '\n', '}']).next()?.trim().trim_matches('"'))
}
/// `hl.monitor({ output = ..., mode = "WxH@Hz", position = "XxY", scale = ..,
/// transform = .. })` — one call per monitor, with explicit geometry.
///
/// The counterpart to [`parse_mon_row`], which derives positions by laying
/// monitors out in a row. A profile reaches for this form precisely when the
/// row is wrong: a portrait monitor centred against a shorter one cannot be
/// expressed as a row, so the positions are written out.
pub fn parse_hl_monitor(text: &str) -> (Vec<Monitor>, Vec<String>) {
    let mut out = vec![];
    let mut warnings = vec![];
    let mut rest = text;
    while let Some(start) = rest.find("hl.monitor(") {
        let tail = &rest[start + "hl.monitor(".len()..];
        let Some(open) = tail.find('{') else {
            rest = tail;
            continue;
        };
        let Some(close) = tail[open..].find('}') else {
            rest = tail;
            continue;
        };
        let group = &tail[open + 1..open + close];
        rest = &tail[open + close..];

        let Some(output) = field(group, "output") else {
            warnings.push("hl.monitor without an output".into());
            continue;
        };
        if field(group, "disable").is_some_and(|v| v == "true") {
            out.push(Monitor {
                output: output.into(),
                enabled: false,
                ..Monitor::default()
            });
            continue;
        }
        // Keywords mean "let the compositor decide", same as the conf dialect.
        let mode = match field(group, "mode") {
            None | Some("preferred") | Some("highres") | Some("highrr") => None,
            Some(raw) => match Mode::parse(raw) {
                Some(mode) => Some(mode),
                None => {
                    warnings.push(format!("unparseable hl.monitor mode {raw:?}"));
                    continue;
                }
            },
        };
        let position = match field(group, "position") {
            None => None,
            Some(raw) if raw.starts_with("auto") => None,
            Some(raw) => match raw.split_once('x') {
                Some((x, y)) => match (x.trim().parse::<i32>(), y.trim().parse::<i32>()) {
                    (Ok(x), Ok(y)) => Some((x, y)),
                    _ => {
                        warnings.push(format!("unparseable hl.monitor position {raw:?}"));
                        continue;
                    }
                },
                None => {
                    warnings.push(format!("unparseable hl.monitor position {raw:?}"));
                    continue;
                }
            },
        };
        let scale = match field(group, "scale") {
            None | Some("auto") => 1.0,
            Some(raw) => match raw.parse() {
                Ok(scale) => scale,
                Err(_) => {
                    warnings.push(format!("unparseable hl.monitor scale {raw:?}"));
                    continue;
                }
            },
        };
        out.push(Monitor {
            output: output.into(),
            mode,
            scale,
            position,
            transform: field(group, "transform")
                .and_then(|x| x.parse().ok())
                .unwrap_or(0),
            enabled: true,
        })
    }
    (out, warnings)
}

pub fn parse_mon_row(text: &str) -> (Vec<Monitor>, Vec<String>) {
    let Some(start) = text.find("mon.row(") else {
        return (vec![], vec![]);
    };
    let tail = &text[start + 8..];
    let mut out = vec![];
    let mut begins = Vec::new();
    for (i, c) in tail.char_indices() {
        if c == '{' {
            begins.push(i + 1);
        } else if c == '}'
            && let Some(begin) = begins.pop()
        {
            let g = &tail[begin..i];
            if g.contains('{') {
                continue;
            }
            let parsed = (
                field(g, "output"),
                field(g, "w"),
                field(g, "h"),
                field(g, "hz"),
                field(g, "scale"),
            );
            if let (Some(o), Some(w), Some(h), Some(hz), Some(scale)) = parsed
                && let (Ok(width), Ok(height), Ok(refresh), Ok(scale)) =
                    (w.parse(), h.parse(), hz.parse(), scale.parse())
            {
                let transform = field(g, "transform")
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0);
                out.push(Monitor {
                    output: o.into(),
                    mode: Some(Mode {
                        width,
                        height,
                        refresh,
                    }),
                    scale,
                    position: None,
                    transform,
                    enabled: true,
                })
            }
        }
    }
    (out, vec![])
}
pub fn parse_workspace_rules(text: &str) -> Vec<WorkspaceRule> {
    let mut out = vec![];
    for line in text.lines() {
        let t = line.trim();
        if let Some(rhs) = t
            .strip_prefix("workspace")
            .and_then(|x| x.trim_start().strip_prefix('='))
            .map(str::trim)
        {
            let p: Vec<_> = rhs.split(',').map(str::trim).collect();
            if p.len() >= 2
                && let Some(m) = p[1].strip_prefix("monitor:")
            {
                out.push(WorkspaceRule {
                    workspace: p[0].into(),
                    monitor: m.into(),
                    default: p.contains(&"default:true"),
                })
            }
        } else if t.contains("workspace_rule")
            && let (Some(ws), Some(mon)) = (field(t, "workspace"), field(t, "monitor"))
        {
            out.push(WorkspaceRule {
                workspace: ws.into(),
                monitor: mon.into(),
                default: field(t, "default").is_some_and(|x| x == "true"),
            })
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directive_key_charsets() {
        assert_eq!(parse_directive("#@ battery-low = x", false), None);
        assert_eq!(
            parse_directive("#@ battery-low = x", true),
            Some(("battery-low", "x"))
        );
        assert_eq!(parse_directive("#@match=x", false), Some(("match", "x")));
        assert_eq!(parse_directive("#@ match = ", false), None);
        assert_eq!(parse_directive("#@ Match = x", false), None)
    }
    #[test]
    fn directive_lua_leader() {
        assert_eq!(
            parse_directive("--@ match = desc:X", false),
            Some(("match", "desc:X"))
        );
        assert_eq!(parse_directive("-- match = x", false), None)
    }
    #[test]
    fn directives_stop_at_body() {
        let (p, _) =
            to_profile("x", "#@ match = A\nmonitor = A,disable\n#@ edp = disable").unwrap();
        assert_eq!(p.edp, EdpPolicy::Auto)
    }
    #[test]
    fn lua_comments_do_not_end_the_header() {
        let(p,_)=to_profile("x","-- Profile: Dual Dell S2725QC 4K @ 120Hz + built-in, side-by-side, target 150%.\n--\n--@ match = A\n--@ match = B\n--@ edp = auto\nmon.row({})").unwrap();
        assert_eq!(p.matches.len(), 2);
        assert_eq!(
            p.description,
            "Dual Dell S2725QC 4K @ 120Hz + built-in, side-by-side, target 150%."
        )
    }
    #[test]
    fn malformed_header_is_fatal() {
        assert!(to_profile("x", "monitor=A,disable").is_err());
        assert!(to_profile("x", "#@ match=A\n#@ edp=sideways").is_err());
        assert!(to_profile("x", "#@ match=A\n#@ priority=high").is_err())
    }
    #[test]
    fn conf_body_yields_explicit_positions() {
        let (m, _) = parse_conf_monitors(
            "monitor = desc:Dell A,3840x2160@120,0x0,1.5\nmonitor = eDP-2,disable",
        );
        assert_eq!(m[0].position, Some((0, 0)));
        assert_eq!(m[0].scale, 1.5);
        assert!(!m[1].enabled)
    }
    /// Shape taken from a real profile: a portrait monitor centred against a
    /// shorter one, which is exactly the arrangement `mon.row` cannot express
    /// -- so the positions are written out and the row parser finds nothing.
    #[test]
    fn lua_body_reads_explicit_hl_monitor_calls() {
        let text = r#"
-- Profile: HP rotated portrait (left) + Dell (middle) + panel (right).
--@ match = desc:HP Inc. HP E243 CNK7510Y4B
hl.monitor({ output = "desc:HP Inc. HP E243 CNK7510Y4B",  mode = "1920x1080@60",  position = "0x0",      scale = 1,    transform = 1 })
hl.monitor({ output = "desc:Dell Inc. DELL S2721HGF 85YF", mode = "1920x1080@144", position = "1080x420", scale = 1 })
hl.monitor({ output = "eDP-2",                             mode = "2560x1600@165", position = "3000x420", scale = 1.25 })
"#;
        let (profile, warnings) = to_profile("hp-portrait", text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(profile.monitors.len(), 3, "all three monitors must parse");

        let hp = &profile.monitors[0];
        assert_eq!(hp.transform, 1, "the portrait rotation must survive");
        assert_eq!(hp.position, Some((0, 0)));
        assert_eq!(hp.mode.map(|m| m.refresh), Some(60.0));

        let dell = &profile.monitors[1];
        assert_eq!(
            dell.position,
            Some((1080, 420)),
            "the y offset is the whole reason this profile is not a row"
        );
        assert_eq!(dell.mode.map(|m| m.refresh), Some(144.0));

        assert_eq!(profile.monitors[2].scale, 1.25);
        assert_eq!(profile.monitors[2].position, Some((3000, 420)));
    }

    #[test]
    fn conf_body_accepts_hyprland_keywords() {
        // Real profile line from the reference machine: scale `auto`. A
        // strict numeric parse dropped the monitor entirely and the whole
        // profile migrated empty.
        let text = "monitor = desc:Dell Inc. DELL S3422DWG HSRTS63,3440x1440@144,0x0,auto";
        let (mons, warns) = parse_conf_monitors(text);
        assert_eq!(mons.len(), 1, "keyword scale must not drop the monitor");
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(mons[0].scale, 1.0);
        assert_eq!(mons[0].position, Some((0, 0)));
        assert_eq!(mons[0].mode.map(|m| m.width), Some(3440));
    }

    #[test]
    fn conf_body_accepts_preferred_and_auto_position() {
        let (mons, warns) = parse_conf_monitors("monitor = eDP-2,preferred,auto,1.25");
        assert_eq!(mons.len(), 1);
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(mons[0].mode, None, "preferred means the compositor picks");
        assert_eq!(mons[0].position, None, "auto position is row-derived");
        assert_eq!(mons[0].scale, 1.25);
    }

    #[test]
    fn conf_body_reads_transform() {
        assert_eq!(
            parse_conf_monitors("monitor = desc:Dell A,3840x2160@60,0x0,1,transform,1").0[0]
                .transform,
            1
        )
    }
    #[test]
    fn mon_row_body_yields_targets_without_positions() {
        let s = r#"mon.row({ { output="A", w=3840, h=2160, hz=120, scale=1.5 }, { output="B", w=3840, h=2160, hz=120, scale=1.5 }, { output="eDP-2", w=2560, h=1600, hz=165, scale=1.5 } })"#;
        let (m, _) = parse_mon_row(s);
        assert_eq!(m.len(), 3);
        assert!(m.iter().all(|x| x.position.is_none() && x.scale == 1.5));
        assert_eq!(m[2].mode.unwrap().to_string(), "2560x1600@165")
    }
    #[test]
    fn mon_row_reads_transform() {
        let (m, _) = parse_mon_row(
            r#"mon.row({ { output="A", w=3440, h=1440, hz=144, scale=1, transform=3 } })"#,
        );
        assert_eq!(m[0].transform, 3)
    }
    #[test]
    fn workspace_rules_from_both_dialects() {
        let a = parse_workspace_rules(
            r#"hl.workspace_rule({ workspace = "1", monitor = "desc:X", default = true })"#,
        );
        let b = parse_workspace_rules("workspace = 1, monitor:desc:X, default:true");
        assert_eq!(a, b);
        assert!(a[0].default)
    }
    #[test]
    fn unrecognised_body_warns_but_keeps_header() {
        let (p, w) = to_profile("x", "#@ match = A\nsome_unknown_call()").unwrap();
        assert!(p.monitors.is_empty());
        assert_eq!(w.len(), 1)
    }
}
