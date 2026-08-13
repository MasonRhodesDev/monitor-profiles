//! CLI scaffold for the shared monitor-profiles config tool (#3).
//!
//! Layouts live in `/etc/monitor-profiles` (shared) and optional
//! `~/.config/hypr/profiles/*.toml` (per-user overrides) — not in this repo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use monitor_profiles::{from_toml, legacy, load_dir, render, to_toml};

#[derive(Parser)]
#[command(name = "monitor-profiles", about = "Neutral monitor layout profiles")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List profile stems from system and/or user directories.
    List {
        /// Only `/etc/monitor-profiles`.
        #[arg(long)]
        system: bool,
        /// Only `~/.config/hypr/profiles`.
        #[arg(long)]
        user: bool,
    },
    /// Print a profile's TOML (searches system then user).
    Show { name: String },
    /// Parse and report diagnostics. Non-zero on errors.
    Validate {
        /// File or directory (default: `/etc/monitor-profiles`).
        path: Option<PathBuf>,
    },
    /// Convert a legacy `.conf`/`.lua` file to TOML on stdout.
    Migrate { path: PathBuf },
    /// Emit Hyprland lua and conf for a named profile (debug/CI).
    Render { name: String },
}

fn system_dir() -> PathBuf {
    PathBuf::from("/etc/monitor-profiles")
}

fn user_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hypr/profiles")
}

fn stems_in(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return names;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("toml") {
            continue;
        }
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            if !stem.starts_with('.') {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    names
}

fn find_toml(name: &str) -> Option<PathBuf> {
    for dir in [user_dir(), system_dir()] {
        let p = dir.join(format!("{name}.toml"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::List { system, user } => {
            let both = !system && !user;
            if system || both {
                println!("# system ({})", system_dir().display());
                for n in stems_in(&system_dir()) {
                    println!("{n}");
                }
            }
            if user || both {
                println!("# user ({})", user_dir().display());
                for n in stems_in(&user_dir()) {
                    println!("{n}");
                }
            }
            ExitCode::SUCCESS
        }
        Cmd::Show { name } => {
            let Some(path) = find_toml(&name) else {
                eprintln!("error: profile {name:?} not found in user or system dirs");
                return ExitCode::FAILURE;
            };
            match fs::read_to_string(&path) {
                Ok(text) => {
                    print!("{text}");
                    if !text.ends_with('\n') {
                        println!();
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {}: {e}", path.display());
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Validate { path } => {
            let path = path.unwrap_or_else(system_dir);
            if path.is_file() {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("profile");
                let text = match fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("error: {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                };
                match from_toml(name, &text) {
                    Ok((_, warnings)) => {
                        for w in warnings {
                            eprintln!("warning: {name}: {w}");
                        }
                        println!("ok {}", path.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {name}: {e}");
                        ExitCode::FAILURE
                    }
                }
            } else {
                let (profiles, diags) = load_dir(&path);
                let mut failed = !diags.is_empty();
                for d in &diags {
                    eprintln!("error: {}: {}", d.source, d.message);
                }
                for p in &profiles {
                    if p.monitors.is_empty() {
                        eprintln!("error: {}: no monitors", p.name);
                        failed = true;
                    } else {
                        println!("ok {}", p.name);
                    }
                }
                if failed {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
        }
        Cmd::Migrate { path } => {
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("profile");
            match legacy::to_profile(name, &text) {
                Ok((profile, warnings)) => {
                    for w in warnings {
                        eprintln!("warning: {w}");
                    }
                    if profile.monitors.is_empty() {
                        eprintln!("error: conversion produced zero monitors");
                        return ExitCode::FAILURE;
                    }
                    print!("{}", to_toml(&profile));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Render { name } => {
            let Some(path) = find_toml(&name) else {
                eprintln!("error: profile {name:?} not found");
                return ExitCode::FAILURE;
            };
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            let (profile, warnings) = match from_toml(&name, &text) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            for w in warnings {
                eprintln!("warning: {w}");
            }
            let (lua, lw) = render::render_lua(&profile);
            let (conf, cw) = render::render_conf(&profile);
            for w in lw.into_iter().chain(cw) {
                eprintln!("warning: {w}");
            }
            println!("===== {}.lua =====", profile.name);
            print!("{lua}");
            println!("===== {}.conf =====", profile.name);
            print!("{conf}");
            ExitCode::SUCCESS
        }
    }
}
