# Development

MIR3 Studio AI is a **Tauri 2 + React 19** app: the UI lives in `src/`, the Rust backend in `src-tauri/`.

## Requirements

| Tool | Version |
| --- | --- |
| Node.js | 22.15+ / 23.8+ / 24+ |
| Rust | 1.77.2+ |
| pnpm | 10.28.2 |

Plus the platform toolchain:

- **Windows** — MSVC build tools + WebView2
- **macOS** — Xcode Command Line Tools
- **Linux** — WebKit2GTK

## Commands

```bash
pnpm install      # install dependencies
pnpm dev          # frontend dev server (Vite)
pnpm typecheck    # frontend TypeScript check
pnpm tauri dev    # run the desktop app in debug mode
pnpm package:mac  # canonical macOS package and verification gate
```

`pnpm package:mac` downloads, verifies, and embeds the target platform runtime described by `runtime-baseline.lock.json`, then builds and verifies the macOS app and DMG. Production builds accept only an `approved` platform. A `testing` candidate requires the explicit `MIR3_BASELINE_ALLOW_UNVALIDATED=1` override and may only be used to collect real-platform validation evidence. Any Core, Node.js, pnpm, URL, or SHA-256 change restarts validation for that platform. See the [runtime baseline policy](./runtime-baseline-policy.md).

After a functional change, run `pnpm version:bump -- patch` to synchronize the product version, then `pnpm release:check`. Bundled plugins also need their own SemVer bump and local changelog, and must follow the [Harness plugin development policy](./harness-plugin-development-policy.md). Commit and push the current Git branch after all checks pass.

Backend checks (from `src-tauri/`):

```bash
cargo check
cargo test
```

## Tips

- Debug mode serves on port **3081**, release builds on **3080** — the two never clash, so you can run an installed copy and a dev build side by side.
