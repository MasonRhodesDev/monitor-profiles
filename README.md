# monitor-profiles

`monitor-profiles` is a neutral source of truth for monitor layouts shared between a compositor session manager and a login screen, because neither component can read the other's configuration dialect.

## Format

- `description`: string, default `""`. Human-readable profile summary.
- `match`: array of strings, required and non-empty. EDID description prefixes that must all be connected.
- `edp`: `"auto" | "enable" | "disable"`, default `"auto"`. Controls whether the internal panel follows lid state or is pinned.
- `gpu`: `"auto" | "igpu" | "dgpu"`, default `"auto"`. Selects the preferred render GPU.
- `hooks`: array of strings, default `[]`. Session hooks retained by the neutral model.
- `priority`: integer, default number of `match` entries. Overrides profile selection precedence.
- `[[monitor]]`: repeated monitor table.
  - `output`: string, required. A connector name or `desc:` EDID-description prefix.
  - `mode`: string, default preferred mode. A `WIDTHxHEIGHT` mode with optional `@REFRESH`.
  - `scale`: positive number, default `1.0`. This is a **target** and is snapped to the nearest compositor-valid fractional scale for the mode.
  - `position`: two-integer array, optional. Explicit logical `[x, y]`; by default positions are derived left-to-right as a row.
  - `transform`: integer `0..=7`, default `0`. Wayland output transform; odd transforms are portrait.
  - `enabled`: boolean, default `true`. Whether the output is enabled.
- `[[workspace]]`: repeated workspace table.
  - `workspace`: string, required. Workspace identifier.
  - `monitor`: string, required. Monitor selector to pin it to.
  - `default`: boolean, default `false`. Marks the workspace as the default on that monitor.

## Worked example

`dual-4k.toml`:

```toml
description = "Dual Dell S2725QC 4K @ 120Hz + built-in, side-by-side, target 150%."
match = [
    "desc:Dell Inc. DELL S2725QC 5DGMS84",
    "desc:Dell Inc. DELL S2725QC FFJMS84",
]
edp = "auto"

# Scales are targets: 1.5 is invalid for the 2560x1600 panel, so it resolves
# to 1.6 (the nearest scale yielding an integer logical size). Positions are
# derived left-to-right from logical widths — 0, 2560, 5120.
[[monitor]]
output = "desc:Dell Inc. DELL S2725QC 5DGMS84"
mode = "3840x2160@120"
scale = 1.5

[[monitor]]
output = "desc:Dell Inc. DELL S2725QC FFJMS84"
mode = "3840x2160@120"
scale = 1.5

[[monitor]]
output = "eDP-2"
mode = "2560x1600@165"
scale = 1.5

[[workspace]]
workspace = "1"
monitor = "desc:Dell Inc. DELL S2725QC 5DGMS84"
default = true
```

## Matching

All profile matches must hit. Each hit is a prefix match against an EDID description (with an optional `desc:` prefix). Eligible profiles tie-break by priority, then match count, then name, all descending.

## Usage

```rust
use std::path::Path;
use monitor_profiles::{load_dir, resolve, select, ConnectedOutput};

let (profiles, diagnostics) = load_dir(Path::new("profiles"));
let connected: Vec<ConnectedOutput> = obtain_connected_outputs();
let signature = connected.iter().map(|o| o.description.clone()).collect::<Vec<_>>();
if let Some(profile) = select(&signature, &profiles) {
    let layout = resolve(profile, &connected);
    apply(layout);
}
```

The `hyprland-render` feature generates Hyprland artifacts and is off by default.

## Migration

`legacy::to_profile` reads the old `#@` and `--@` directive dialects. `.conf` bodies convert cleanly; `mon.row` Lua bodies convert through a best-effort scan. Any other Lua body must be converted by hand.

## License

MIT.
