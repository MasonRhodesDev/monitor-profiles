//! The profile model. Ported from hyprstate's `hyprstate-fsm::profiles`,
//! with geometry promoted into the model.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdpPolicy {
    #[default]
    Auto,
    Enable,
    Disable,
}
impl EdpPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Enable => "enable",
            Self::Disable => "disable",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "enable" => Some(Self::Enable),
            "disable" => Some(Self::Disable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuPref {
    #[default]
    Auto,
    Igpu,
    Dgpu,
}
impl GpuPref {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Igpu => "igpu",
            Self::Dgpu => "dgpu",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "igpu" => Some(Self::Igpu),
            "dgpu" => Some(Self::Dgpu),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: f64,
}
impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        let (dims, refresh) = match s.split_once('@') {
            Some((d, r)) => (d, r.trim().parse().ok()?),
            None => (s, 0.0),
        };
        let (w, h) = dims.trim().split_once('x')?;
        let (width, height) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
        (width > 0 && height > 0).then_some(Self {
            width,
            height,
            refresh,
        })
    }
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.refresh > 0.0 {
            write!(
                f,
                "{}x{}@{}",
                self.width,
                self.height,
                fmt_num(self.refresh)
            )
        } else {
            write!(f, "{}x{}", self.width, self.height)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub output: String,
    pub mode: Option<Mode>,
    pub scale: f64,
    pub position: Option<(i32, i32)>,
    pub transform: u8,
    pub enabled: bool,
}
impl Default for Monitor {
    fn default() -> Self {
        Self {
            output: String::new(),
            mode: None,
            scale: 1.0,
            position: None,
            transform: 0,
            enabled: true,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceRule {
    pub workspace: String,
    pub monitor: String,
    pub default: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub matches: Vec<String>,
    pub edp: EdpPolicy,
    pub gpu: GpuPref,
    pub hooks: Vec<String>,
    pub priority: i64,
    pub monitors: Vec<Monitor>,
    pub workspaces: Vec<WorkspaceRule>,
}

pub fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mode_parses_with_and_without_refresh() {
        assert_eq!(
            Mode::parse("3840x2160@120"),
            Some(Mode {
                width: 3840,
                height: 2160,
                refresh: 120.0
            })
        );
        assert_eq!(
            Mode::parse("2560x1600"),
            Some(Mode {
                width: 2560,
                height: 1600,
                refresh: 0.0
            })
        );
    }
    #[test]
    fn mode_rejects_garbage() {
        for s in ["", "1920", "0x0", "axb"] {
            assert_eq!(Mode::parse(s), None);
        }
    }
    #[test]
    fn mode_display_round_trips() {
        assert_eq!(
            Mode::parse("3840x2160@120").unwrap().to_string(),
            "3840x2160@120"
        );
        assert_eq!(Mode::parse("2560x1600").unwrap().to_string(), "2560x1600");
    }
    #[test]
    fn fmt_num_trims() {
        assert_eq!(fmt_num(165.0), "165");
        assert_eq!(fmt_num(1.25), "1.25");
        assert_eq!(fmt_num(1.5), "1.5");
    }
}
