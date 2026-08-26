<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">An AI desktop development workspace for 996 MIR3 engine developers</p>

<p align="center"><samp><strong>English</strong> · <a href="./README.md">中文</a></samp></p>

> Version `0.9.3` develops projects created by 996 Project Manager through one system kernel, 33 domain packs, archived system sessions, and a safe Draft workflow.

MIR3 Studio AI uses DeepSeek Harness as one of its open-source Agent infrastructure foundations. On top of its plugin architecture, we independently develop the project model, knowledge system, development toolchain, and AI workflows for the 996 MIR3 ecosystem.

## Capabilities

- Native Tauri 2 + React desktop shell for Windows, macOS, and Linux.
- Installers carry a locked Node.js, pnpm, and MIR3 AI Core baseline for the target platform, so first launch does not download the Core from GitHub.
- Core version downloads, switching, health checks, and process lifecycle management.
- Isolated profiles, plugin management, and recovery workflows.
- 996 project recognition, external indexing, real file-to-resource mapping, governed knowledge, Draft previews, and version snapshots.
- A three-pane system workspace: real files/resources/dependencies, domain views/Diff/validation, and an archived AI session.
- One system kernel and 33 independently versioned domain packs that can be audited, upgraded, disabled, and rolled back without competing for the Harness UI lifecycle.
- Twelve fixed MCP tools expose the same resources, dependencies, Draft diffs, validation, and capability registry to system AI and the global Harness workbench.
- Successful tasks produce Task Receipts and can be promoted, after preview and validation, into versioned project, personal, or team capabilities.
- Studio domain-source editing first opens an external Draft bound to the current system version, then performs format-preserving TXT/Lua changes and BIFF XLS viewing; Harness AI writes only through the task-scoped generic MCP.
- Self-updates exclusively from MIR3 Studio AI GitHub Releases.

## Public identity

| Item | Value |
| --- | --- |
| App | MIR3 Studio AI |
| Core display name | MIR3 AI Core |
| Version | 0.9.3 |
| Tauri identifier | `ai.mir3.studio` |
| Data directory | `~/.mir3-studio-ai` |
| Development data directory | `~/.mir3-studio-ai.dev` |
| Data override | `MIR3_STUDIO_HOME` |

## Quick start

Download the installer for your platform from [GitHub Releases](https://github.com/shenmuli0324-glitch/mir3-studio-ai/releases). First launch verifies and installs the bundled runtime baseline; network access is only needed for later update checks.

## Development

```bash
corepack pnpm install
corepack pnpm tauri dev
```

Use `pnpm package:mac` for the fixed Apple Silicon macOS delivery flow. It builds the `.app` and `.dmg`, verifies the app signature and disk image, and prints the SHA-256 digest. Without a Developer ID, it defaults to an ad-hoc signature suitable for local device testing.

Production builds that enable remote domain-pack candidates must inject both the HTTPS index URL through `MIR3_DOMAIN_PACK_INDEX_URL` and a Base64-encoded 32-byte Ed25519 public key through `MIR3_DOMAIN_PACK_ED25519_PUBLIC_KEY` at compile time. The build rejects one-sided configuration, non-HTTPS or credential-bearing URLs, and clearly invalid keys. When configured, Studio checks after 60 seconds and every six hours, verifies signatures, and stages candidates in the background, but never activates them without user confirmation. Without configuration the background job stays disabled and manual remote checks fail closed with `DOMAIN_PACK_UPDATE_NOT_CONFIGURED`; bundled/local candidates, confirmed activation, and rollback remain available. The repository intentionally contains no placeholder official source or key.

All 33 domain packs use evidence-gated engine generalization. Only SemVer, v-prefixed SemVer, and major.minor aliases are normalized, and write access also requires the real 996 project layout, a domain selector/content fingerprint, and resource-schema validation. Unknown or incompatible engines remain viewable for diagnostics but are refused by both Draft writes and final Apply.

See [Development](./docs/DEVELOPMENT.md), the [runtime baseline policy](./docs/runtime-baseline-policy.md), and the product [CHANGELOG](./CHANGELOG.md).

## Data and privacy

MIR3 Studio AI does not read, migrate, or delete another product's data directory. The only external data override is `MIR3_STUDIO_HOME`; required compatibility environment mapping happens only inside the spawned core process.

## Third-party software and license

See [LICENSE](./LICENSE), [LICENSE.details](./LICENSE.details), and [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
