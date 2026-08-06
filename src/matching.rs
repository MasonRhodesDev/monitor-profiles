//! Profile selection using prefix matching.
use crate::model::Profile;
pub fn match_in_signature(m: &str, signature: &[String]) -> bool {
    let needle = m.strip_prefix("desc:").unwrap_or(m).trim();
    signature.iter().any(|d| d.starts_with(needle))
}
pub fn select<'a>(signature: &[String], profiles: &'a [Profile]) -> Option<&'a Profile> {
    profiles
        .iter()
        .filter(|p| p.matches.iter().all(|m| match_in_signature(m, signature)))
        .max_by(|a, b| {
            (a.priority, a.matches.len(), &a.name).cmp(&(b.priority, b.matches.len(), &b.name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdpPolicy, GpuPref};
    fn prof(name: &str, ms: &[&str], p: i64) -> Profile {
        Profile {
            name: name.into(),
            description: String::new(),
            matches: ms.iter().map(|x| (*x).into()).collect(),
            edp: EdpPolicy::Auto,
            gpu: GpuPref::Auto,
            hooks: vec![],
            priority: p,
            monitors: vec![],
            workspaces: vec![],
        }
    }
    #[test]
    fn select_requires_all_matches() {
        let s = vec!["Dell U2723QE ABC123".into(), "BOE 0x0BCA".into()];
        let p = vec![prof("both", &["Dell U2723QE", "BOE"], 2)];
        assert_eq!(select(&s, &p).unwrap().name, "both");
        assert!(select(&["BOE 0x0BCA".into()], &p).is_none());
    }
    #[test]
    fn select_specificity_then_explicit_priority() {
        let s = vec!["Dell A".into(), "BOE B".into()];
        let mut p = vec![prof("one", &["Dell"], 1), prof("two", &["Dell", "BOE"], 2)];
        assert_eq!(select(&s, &p).unwrap().name, "two");
        p.push(prof("pinned", &["Dell"], 99));
        assert_eq!(select(&s, &p).unwrap().name, "pinned");
    }
    #[test]
    fn match_strips_desc_prefix_and_uses_startswith() {
        let s = vec!["Dell U2723QE HJKL (DP-3)".into()];
        assert!(match_in_signature("desc:Dell U2723QE", &s));
        assert!(match_in_signature("Dell U2723QE", &s));
        assert!(!match_in_signature("U2723QE", &s));
    }
    #[test]
    fn select_none_when_nothing_matches() {
        assert!(select(&[], &[prof("x", &["X"], 1)]).is_none());
    }
}
