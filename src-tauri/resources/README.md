# Bundled resources

This directory is bundled into MIR3 Studio AI as `resources/**`.

Runtime data uses the Tauri application identifier `ai.mir3.studio`:

- `runtime/` — managed Node.js runtime
- `dependencies/dsh/` — managed MIR3 AI Core compatibility bundle
- `dependencies/pnpm/` — application-private package manager
- `internal-tools/bin/` — private shims used by core and plugin subprocesses
- `logs/` — desktop and core service logs
- `.store.dat` / `.store.dev.dat` — release and development settings

User projects, profiles, sessions, settings, and plugin state are stored under
`${MIR3_STUDIO_HOME:-$HOME/.mir3-studio-ai}`. Debug builds default to
`$HOME/.mir3-studio-ai.dev`. The desktop does not migrate or delete data from
another application.

## Preset plugins

`preset-plugins.json` drives the first-run and sidebar preset list. Each entry
contains a unique package `id`, install `spec`, display `name`, bilingual
`description`, `repoUrl`, and optional `recommended`, `fix`, and `winOnly`
flags. Plugin source code is not vendored into this repository.
