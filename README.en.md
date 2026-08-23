<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">An AI desktop development workspace for 996 MIR3 engine developers</p>

<p align="center"><samp><strong>English</strong> · <a href="./README.md">中文</a></samp></p>

> Version `0.1.0` establishes the independent product, data, and release identity. It adds no new MIR3-specific features; existing core, plugin, profile, and update capabilities remain available.

## Capabilities

- Native Tauri 2 + React desktop shell for Windows, macOS, and Linux.
- Automatic first-run preparation of Node.js and MIR3 AI Core.
- Core version downloads, switching, health checks, and process lifecycle management.
- Isolated profiles, plugin management, and recovery workflows.
- One public command, `mir3`; plugin pnpm tooling stays private to the app.
- Self-updates exclusively from MIR3 Studio AI GitHub Releases.

## Public identity

| Item | Value |
| --- | --- |
| App | MIR3 Studio AI |
| Core display name | MIR3 AI Core |
| Version | 0.1.0 |
| Tauri identifier | `ai.mir3.studio` |
| CLI | `mir3` |
| Data directory | `~/.mir3-studio-ai` |
| Development data directory | `~/.mir3-studio-ai.dev` |
| Data override | `MIR3_STUDIO_HOME` |

## Quick start

Download the installer for your platform from [GitHub Releases](https://github.com/shenmuli0324-glitch/mir3-studio-ai/releases). The first launch needs network access to prepare the runtime; afterward the core service runs locally on the loopback interface.

## Development

```bash
corepack pnpm install
corepack pnpm tauri dev
```

See [Development](./docs/DEVELOPMENT.md) for details.

## Data and privacy

MIR3 Studio AI does not read, migrate, or delete another product's data directory. The only external data override is `MIR3_STUDIO_HOME`; required compatibility environment mapping happens only inside the spawned core process.

## Third-party software and license

See [LICENSE](./LICENSE), [LICENSE.details](./LICENSE.details), and [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
