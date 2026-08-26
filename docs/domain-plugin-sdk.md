# Domain Plugin SDK

MIR3 Studio publishes the bundled `@mir3-studio/domain-plugin-sdk` as the identifiable contract for all 33 versioned domain plugins. A domain plugin is data, schemas, fixtures, documentation, and registered safe operations; it is not an executable Harness plugin.

## Entrypoints

- `@mir3-studio/domain-plugin-sdk`: `defineDomainManifest` and `defineDomainFixtures`.
- `@mir3-studio/domain-plugin-sdk/contract`: validation-only CI entrypoint.
- `scripts/generate-domain-packs.mjs`: repository generator that consumes the same SDK.
- `mir3_domain::execute_domain_pack_fixture_canary`: authoritative Rust runtime fixture runner.

## Required package layout

```text
domain.json
schemas/resource.schema.json
fixtures/valid.json
fixtures/invalid.json
fixtures/expected-diagnostics.json
README.md
CHANGELOG.md
package.json
```

Only registered `text`, `xls`, `map`, `graph`, and `timeline` primitives may appear in operation steps; central presentations use the runtime-supported `xls`, `map`, `graph`, or `timeline` primitives. Unknown formats remain read-only. Every write capability requires a closed parameter schema, an external Draft, expected revision, reversible structured steps, preview, validation, and user confirmation before project application. The SDK also checks schema versions, Kernel primitives, file projections, stable resource identity, dependency scope, validators, documentation, and fixture paths. It never loads domain-provided JavaScript, React components, native binaries, shell commands, or arbitrary filesystem writers.

`src-tauri/resources/mir3-domain-sdk/fixtures/example-pack` is installable through the same candidate validation path as a bundled `level` package. `fixtures/contract-corpus.json` defines accepted and rejected mutations that both the JavaScript SDK audit and Rust runtime tests must agree on.

Run `pnpm domain:audit` and the `mir3-domain` fixture tests before distributing a package. Domain package activation still uses Studio candidate verification, hash/signature checks, and current/previous/LKG rollback.
