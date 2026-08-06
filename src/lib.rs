//! Neutral monitor layout profiles.
//!
//! A profile is a TOML file: a set of `match` signatures identifying a
//! display arrangement, plus the geometry to apply when it matches. The
//! format is compositor-agnostic on purpose — a session manager and a login
//! screen must be able to produce the same arrangement without sharing a
//! compositor, or a config dialect, between them.
//!
//! Everything here is pure except [`parse::load_dir`]. This crate feeds a
//! login screen: a malformed profile is skipped with a diagnostic, never a
//! panic and never a failed load.

pub mod layout;
pub mod legacy;
pub mod matching;
pub mod model;
pub mod parse;
#[cfg(feature = "hyprland-render")]
pub mod render;

pub use layout::{
    ConnectedOutput, ResolvedLayout, ResolvedOutput, resolve, resolve_all, valid_scale,
};
pub use matching::{match_in_signature, select};
pub use model::{EdpPolicy, GpuPref, Mode, Monitor, Profile, WorkspaceRule};
pub use parse::{Diagnostic, from_toml, load_dir};
