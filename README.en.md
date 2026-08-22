<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">
  An AI desktop development environment for 996 MIR3 engine developers —<br />
  powered by <a href="https://github.com/deepseek-ai/deepseek-harness">DeepSeek Harness</a>, with the complete Harness, plugin, and update capabilities preserved.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux-black?style=flat-square" alt="Windows | macOS | Linux" />
</p>

<p align="center">
  <samp><strong>English</strong> · <a href="./README.md">中文</a></samp>
</p>

<p align="center">
  <img src="./docs/images/hero-en.png" width="100%" alt="MIR3 Studio AI English interface preview" />
</p>


> [More preview images](./docs/PREVIEW.md)

> This stage covers branding only. It adds no MIR3-specific features and preserves all existing desktop capabilities.

## Features

- ⚡️ **Zero setup** — First launch bootstraps the bundled Node runtime and Harness core automatically; a compatible local Node / Pnpm setup is reused as-is when present.
- 🔄 **Core update** — Every launch syncs with the latest upstream Harness release, so upstream updates reach you without reinstalling; download, switch, and uninstall multiple core versions (auto-restart after switching).
- 🖥️ **Config** — One dialog for Debug / Profiles / Plugins / Core, with a fully localized (zh/en) UI and dark-mode support.
- 🗂️ **Profile isolation** — Create, switch, and remove isolated profiles in the config center; plugins, patches, and settings stay independent per profile.
- 🧩 **Plugin management** — The plugin panel lists installed plugins read-only and offers upgrade / uninstall when one misbehaves, with live error sync.
- 🪶 **Native & lightweight** — A Tauri 2 shell (not Electron): smaller installers, lower memory, native windows. Windows / macOS / Linux, bilingual UI.
- ⌨️ **CLI ready** — Registers `dsh` commands (`*/bin`) after install, ready in a new terminal; never overwrites your existing shell config.
- 🧭 **Launch wizard** — On first launch, pick the recommended plugins (e.g. the dsh-market plugin store) and watch the install stream in real time; skip anytime and reopen later from the sidebar.
- 🚀 **Self-update** — Checks GitHub releases independently and downloads the installer; dev/prod builds are isolated by port and data dir.

## Presets

Plugins offered on the first-run wizard; select what you need and install on demand:

- [DSH Win Terminal Inspector](https://github.com/clearkurt/dsh-win-terminal-inspector) — Windows-only fix for Minimal mode
- [DSH Tauri](https://github.com/hairyf/dsh-tauri) — desktop message bridge: a communication channel with the Tauri 2 shell (Recommended)
- [DSH Market](https://github.com/dsh-market/dsh-market) — the visual plugin market: browse, search, and one-click install community plugins (Recommended)
- [DSH Better Sidebar](https://github.com/omdsh-dev/DSH-better-sidebar) — a VSCode-like right sidebar (explorer/editor/terminal/git/browser), isolated per session (Recommended)
- [DSH Notification](https://github.com/omdsh-dev/dsh-notification) — desktop notifications when a turn finishes: per-outcome toggles plus include/exclude keyword rules
- [DSH Session Context Menu](https://github.com/baihejiangnan/dsh-session-context-menu) — right-click context menu for the DSH app shell: quick actions for conversations, workspaces, inputs, and links

> Want to add new presets? Modify [preset-plugins.json](https://github.com/hairyf/deepseek-harness-desktop/blob/main/src-tauri/resources/preset-plugins.json) and submit a PR.

## Quick Start

Download the MIR3 Studio AI installer for your platform from this project's releases, install, and launch.

The first run downloads the Node runtime and Harness core (~a few hundred MB) and takes you straight into the harness at `http://127.0.0.1:3080`. Everything after that runs locally — no network required.

**Requirements:** Windows 10+ (64-bit) · macOS 10.15+ · Linux (AppImage) · network on first launch

## Dev

Want to get involved in the development? See [docs/DEVELOPMENT.md](./docs/DEVELOPMENT.md).

## How It Works

```text
┌──────────────────────────────────────────────┐
│ Tauri WebView (React)                        │
│   setup state machine → progress → iframe    │
│   loads the dsh web UI + sidebar controls    │
└──────────────────────┬───────────────────────┘
                       │ invoke commands + events
┌──────────────────────┴───────────────────────┐
│ Tauri Rust backend                           │
│   service/download  installer + extraction   │
│   service/core      Harness core versions    │
│   service/profile   dsh profile management   │
│   service/plugin    plugin remove / upgrade  │
│   service/cli       dsh command shim + PATH  │
│   service/update    desktop self-update      │
│   service/workflow  dsh process lifecycle    │
│   task              dsh health checks        │
└──────┬───────────────────────────┬───────────┘
       │                           │
  runtime/ (Node.js v22.22.0)   dependencies/dsh/ (prebuilt bundle)
       └─────────────┬─────────────┘
                     ▼
   dsh --profile <profile> --host 127.0.0.1 --port 3080
                     │  DSH_HOME=~/.dsh
                     ▼
        http://127.0.0.1:3080/  ← embedded UI
```

The prebuilt Harness bundle is published by [deepseek-harness-pkg](https://github.com/hairyf/deepseek-harness-pkg). Every launch diffs the installed bundle against the latest release and re-downloads when outdated — keeping the local install when GitHub is unreachable. A local core installed globally via your package manager (CLI) is preferred when present.

## Notes

> [!WARNING]
> **Developer preview** — upstream `dsh` is evolving fast with breaking changes; this project tracks it closely.

> [!IMPORTANT]
> **macOS Gatekeeper** — the app is not notarized; allow it once via System Settings → Privacy & Security → Open Anyway.

> [!NOTE]
> **Security** — `dsh` can execute code locally. For learning / research / testing only; run it in a trusted, isolated environment.

## Related

- [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) — the upstream `dsh` agent platform
- [deepseek-harness-pkg](https://github.com/hairyf/deepseek-harness-pkg) — prebuilt Harness bundles consumed by this app
- [deepseek-harness-desktop](https://github.com/hairyf/deepseek-harness-desktop) — the upstream desktop foundation for MIR3 Studio AI
- [n8n-desktop](https://github.com/tangtao646/n8n-desktop) — reference implementation

## License

[MIT](./LICENSE) with a [Non-Commercial Condition](./LICENSE.details). The upstream deepseek-harness-desktop copyright notice is preserved.
