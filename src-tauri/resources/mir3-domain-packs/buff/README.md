# buff

MIR3 Studio buff domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
