//! Geometry resolution: fractional-scale snapping and row placement.
use crate::model::{Mode, Profile};
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectedOutput {
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOutput {
    pub name: String,
    pub selector: String,
    pub mode: Option<Mode>,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: u8,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedLayout {
    pub outputs: Vec<ResolvedOutput>,
    pub unmatched: Vec<String>,
    pub warnings: Vec<String>,
}
pub fn valid_scale(width: u32, height: u32, target: f64) -> f64 {
    let (w, h) = (u64::from(width) * 120, u64::from(height) * 120);
    let (mut best, mut bd) = (target, f64::INFINITY);
    for k in 120..=360u64 {
        if w % k == 0 && h % k == 0 {
            let s = k as f64 / 120.0;
            let d = (s - target).abs();
            if d < bd {
                best = s;
                bd = d
            }
        }
    }
    best
}
fn selects(selector: &str, o: &ConnectedOutput) -> bool {
    match selector.strip_prefix("desc:") {
        Some(d) => o.description.starts_with(d.trim()),
        None => o.name == selector,
    }
}
pub fn resolve(profile: &Profile, connected: &[ConnectedOutput]) -> ResolvedLayout {
    let mut l = ResolvedLayout::default();
    let mut x = 0;
    for m in &profile.monitors {
        let Some(o) = connected.iter().find(|o| selects(&m.output, o)) else {
            l.unmatched.push(m.output.clone());
            continue;
        };
        let scale = m
            .mode
            .map_or(m.scale, |z| valid_scale(z.width, z.height, m.scale));
        let position = m.position.unwrap_or((x, 0));
        if m.enabled && m.position.is_none() {
            if let Some(mode) = m.mode {
                let px = if m.transform % 2 == 1 {
                    mode.height
                } else {
                    mode.width
                };
                x += (f64::from(px) / scale + 0.5).floor() as i32
            } else {
                l.warnings.push(format!(
                    "{}: no mode and no explicit position; later monitors may overlap",
                    m.output
                ))
            }
        }
        l.outputs.push(ResolvedOutput {
            name: o.name.clone(),
            selector: m.output.clone(),
            mode: m.mode,
            position,
            scale,
            transform: m.transform,
            enabled: m.enabled,
        })
    }
    l
}
pub fn resolve_all(profile: &Profile) -> ResolvedLayout {
    let c = profile
        .monitors
        .iter()
        .map(|m| ConnectedOutput {
            name: m.output.clone(),
            description: m
                .output
                .strip_prefix("desc:")
                .unwrap_or(&m.output)
                .to_string(),
        })
        .collect::<Vec<_>>();
    resolve(profile, &c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdpPolicy, GpuPref, Monitor};
    fn mon(o: &str, m: &str, s: f64) -> Monitor {
        Monitor {
            output: o.into(),
            mode: Mode::parse(m),
            scale: s,
            ..Monitor::default()
        }
    }
    fn profile(ms: Vec<Monitor>) -> Profile {
        Profile {
            name: "p".into(),
            description: String::new(),
            matches: vec!["A".into()],
            edp: EdpPolicy::Auto,
            gpu: GpuPref::Auto,
            hooks: vec![],
            priority: 1,
            monitors: ms,
            workspaces: vec![],
        }
    }
    #[test]
    fn valid_scale_exact_when_mode_allows() {
        assert_eq!(valid_scale(3840, 2160, 1.5), 1.5);
        assert_eq!(valid_scale(3440, 1440, 1.0), 1.0)
    }
    #[test]
    fn valid_scale_snaps_edp_panel() {
        assert_eq!(valid_scale(2560, 1600, 1.5), 1.6)
    }
    #[test]
    fn valid_scale_exact_at_1_25() {
        assert_eq!(valid_scale(2560, 1600, 1.25), 1.25)
    }
    #[test]
    fn row_derives_x_from_logical_widths() {
        let p = profile(vec![
            mon("A", "3840x2160@120", 1.5),
            mon("B", "3840x2160@120", 1.5),
            mon("eDP-2", "2560x1600@165", 1.5),
        ]);
        let l = resolve_all(&p);
        assert_eq!(
            l.outputs.iter().map(|x| x.position).collect::<Vec<_>>(),
            vec![(0, 0), (2560, 0), (5120, 0)]
        );
        assert_eq!(
            l.outputs.iter().map(|x| x.scale).collect::<Vec<_>>(),
            vec![1.5, 1.5, 1.6]
        )
    }
    #[test]
    fn odd_transform_uses_mode_height_for_advance() {
        let mut b = mon("B", "3840x2160@60", 1.5);
        b.transform = 3;
        let p = profile(vec![
            mon("A", "3440x1440@144", 1.0),
            b,
            mon("C", "1920x1080@60", 1.0),
        ]);
        let l = resolve_all(&p);
        assert_eq!(l.outputs[1].position, (3440, 0));
        assert_eq!(l.outputs[2].position, (4880, 0))
    }
    #[test]
    fn explicit_position_wins_and_does_not_advance() {
        let mut a = mon("A", "1920x1080", 1.0);
        a.position = Some((100, 50));
        let l = resolve_all(&profile(vec![a, mon("B", "1920x1080", 1.0)]));
        assert_eq!(l.outputs[1].position, (0, 0))
    }
    #[test]
    fn disabled_monitor_does_not_advance() {
        let mut b = mon("B", "1920x1080", 1.0);
        b.enabled = false;
        let l = resolve_all(&profile(vec![
            mon("A", "1920x1080", 1.0),
            b,
            mon("C", "1920x1080", 1.0),
        ]));
        assert_eq!(l.outputs[2].position, (1920, 0))
    }
    #[test]
    fn unmatched_selector_is_reported() {
        let l = resolve(&profile(vec![mon("desc:Nope", "1920x1080", 1.0)]), &[]);
        assert!(l.outputs.is_empty());
        assert_eq!(l.unmatched, vec!["desc:Nope"])
    }
    #[test]
    fn selects_by_name_exactly_and_desc_by_prefix() {
        let c = [ConnectedOutput {
            name: "eDP-2".into(),
            description: "BOE 0x0BC9".into(),
        }];
        assert_eq!(
            resolve(&profile(vec![mon("eDP-2", "1920x1080", 1.0)]), &c)
                .outputs
                .len(),
            1
        );
        assert!(
            resolve(&profile(vec![mon("eDP", "1920x1080", 1.0)]), &c)
                .outputs
                .is_empty()
        );
        assert_eq!(
            resolve(&profile(vec![mon("desc:BOE", "1920x1080", 1.0)]), &c)
                .outputs
                .len(),
            1
        )
    }
    #[test]
    fn resolve_all_treats_every_selector_as_present() {
        let l = resolve_all(&profile(vec![
            mon("A", "3840x2160", 1.5),
            mon("B", "3840x2160", 1.5),
            mon("C", "2560x1600", 1.5),
        ]));
        assert_eq!(l.outputs.len(), 3);
        assert!(l.unmatched.is_empty())
    }
}
