# manor

MIR3 Studio manor domain pack for MIR3 System Kernel v1. Pack version: `1.3.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `manorId`: string
- `mapId`: string → map
- `entryNpcId`: string → npc
- `minimumLevel`: integer
- `productionPointId`: string → resource_production

Unique key: `manorId`. Runtime rule: `manor.entry-and-exit-loop-reachable`.

## Capabilities

- `inspect-manor` via `map`
- `clone-manor` via `map`
- `edit-manor-entrance` via `map`
- `validate-manor-loop` via `map`
- `add-manor` via `map`
- `batch-update-manor` via `map`
- `replace-manor-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
