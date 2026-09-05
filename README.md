<h1 align="center">Lumen</h1>
<p align="center">A native status bar for Wayland compositors, forked from <a href="https://github.com/MalpenZibo/ashell">ashell</a>.</p>
<p align="center">
    <a href="https://github.com/yesvus/lumen/blob/main/LICENSE"><img alt="GitHub License" src="https://img.shields.io/github/license/yesvus/lumen"></a>
    <a href="https://github.com/yesvus/lumen/issues"><img alt="GitHub Issues" src="https://img.shields.io/github/issues/yesvus/lumen"></a>
</p>

Lumen started as a personal fork of [ashell](https://github.com/MalpenZibo/ashell) and has since
diverged with its own native modules and UI direction (a real Arch menu popup, a native
Colonnade/niri-style tab strip, ongoing work on a native app launcher and niri-matched
shadows/borders — see [open issues](https://github.com/yesvus/lumen/issues)). It still shares
ashell's core architecture (Elm-style app, iced/iced_layershell, the same compositor backends),
so most of ashell's own documentation still applies unless noted otherwise below.

## ✨ Features

- Automatic Wayland compositor detection (Hyprland, Niri, generic Wayland fallback)
- Multi-monitor support (all monitors, active monitor, or specific targets)
- Hot-reload configuration (changes apply automatically via file watch)
- Bar positioning (top or bottom) with configurable layer (Bottom, Top, Overlay)
- Theming: transparent (islands) or solid bar surface with custom radius, margin, colors, opacity, scale, and fonts
- OS Updates indicator with configurable check interval, and one-click apply
- Active Window (title, class, or initial title/class)
- **Colonnade** — native niri-style column tab strip module
- Workspaces with naming, color coding, and per-monitor visibility
- System Information (CPU, RAM, Disk, IP address, Network speed, Temperature) with warn/alert thresholds
- Keyboard Layout with custom labels (Hyprland/Niri/MangoWC)
- Keyboard Submap (Hyprland/MangoWC)
- System Tray with context menus
- **Arch menu** — native popup with About/Settings/Updates/power entries (replaces the old fuzzel-dmenu shim)
- Clock with calendar, weather, timezone cycling, and format cycling (Tempo)
- Privacy indicators (microphone, camera, and screenshare usage)
- Media Player with album art and track info
- Notification manager with toast popups, grouping, and urgency support
- Settings panel
  - Power menu (shutdown, suspend, hibernate, reboot, logout, lock)
  - Battery and peripheral battery information
  - Audio sources and sinks (with microphone)
  - Screen brightness
  - Network (WiFi scanning, password entry; supports NetworkManager and IWD backends)
  - VPN
  - Bluetooth
  - Power profiles
  - Idle inhibitor
  - Airplane mode
  - Custom quick-action buttons with status commands
- IPC socket for scripting and keybindings (`lumen msg <command>`)
- OSD overlay for volume, brightness, and airplane mode changes
- Custom Modules
  - Button (execute command on click)
  - Text (display-only, update UI with command output via `listen_cmd`)
  - Regex-based icon mapping and alert states

## 🛠️ Install

Not packaged anywhere — this is a personal fork, not a general-purpose release. Build from source:

```bash
make build    # cargo build --release
make install  # install binary to /usr/bin (requires sudo)
```

See `AGENTS.md` for system dependencies (libxkbcommon, libwayland, libpipewire-0.3, libpulse, dbus,
udev, pkg-config, clang/llvm) and other build notes.

## ⚙️ Configuration

Lumen comes with its own default configuration (`Config::default()`), separate from ashell's
upstream defaults. If you want to customize it, ashell's
[Configuration docs](https://malpenzibo.github.io/ashell/docs/configuration) are still a good
reference for most options, since the config schema is largely shared — check `src/config.rs`
for anything Lumen-specific (native module configs like Colonnade/ArchMenu aren't in upstream).

## 📖 Developer Guide

See `AGENTS.md` in the repo root for project structure, build notes, commit/branch conventions,
and the AI-assisted contribution policy. ashell's own
[Developer Guide](https://malpenzibo.github.io/ashell/dev-guide/) is a useful secondary reference
for the parts of the architecture this fork hasn't diverged from yet.

## 🤖 AI-Assisted Contributions

AI-assisted contributions are accepted — the same quality standards apply regardless of how
the code was written. **You are responsible for the code you submit**: review AI output carefully,
ensure `make check` passes, and be prepared to explain your changes.

Before working on a feature or large change, discuss it first (open or comment on a
[GitHub issue](https://github.com/yesvus/lumen/issues)). Small, incremental PRs are preferred.

See `docs/src/contributing/ai-assisted-contributions.md` for the full policy.
