# MIR3 Domain Plugin SDK

`@mir3-studio/domain-plugin-sdk` is the public, declarative authoring entrypoint for MIR3 Studio domain plugins. It targets Kernel API v1 and accepts JSON-compatible manifests and fixture corpora only.

```js
import { defineDomainFixtures, defineDomainManifest } from '@mir3-studio/domain-plugin-sdk'

const manifest = defineDomainManifest(domainJson)
const fixtures = defineDomainFixtures({ valid, invalid, expectedDiagnostics })
```

The SDK validates the complete runtime manifest: all schema versions, required Kernel primitives, real-file projection, resource identity, presentation, dependency scope, structured reversible operations, validators, fixture paths, and Draft preview/validation/confirmation gates. Operation steps use only the registered `text`, `xls`, `map`, `graph`, and `timeline` primitives; presentation primitives use `xls`, `map`, `graph`, or `timeline`. It rejects functions, executable modules, shell commands, arbitrary components, and free code.

Use the `./contract` export in CI for manifest and fixture validation. `fixtures/example-pack` is a complete runtime-installable example and `fixtures/contract-corpus.json` is consumed by both SDK and Rust acceptance tests. The repository generator at `scripts/generate-domain-packs.mjs` uses this same entrypoint, while the Rust `mir3-domain` fixture runner remains the authoritative runtime contract test.

See `docs/domain-plugin-sdk.md` for package layout and release requirements.
