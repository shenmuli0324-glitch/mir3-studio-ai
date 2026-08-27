# buff

MIR3 Studio buff domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `buffId`: string
- `stackMode`: string
- `maximumStacks`: integer
- `durationMilliseconds`: integer
- `effectSkillId`: string → skill

Unique key: `buffId`. Runtime rule: `buff.stack-mode-capacity-compatible`.

## Capabilities

- `inspect-buff` via `timeline`
- `clone-buff` via `timeline`
- `edit-buff-stacking` via `timeline`
- `add-buff` via `timeline`
- `batch-update-buff` via `timeline`
- `replace-buff-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
