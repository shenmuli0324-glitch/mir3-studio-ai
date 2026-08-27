# monster

MIR3 Studio monster domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `monsterId`: string
- `combatLevel`: integer
- `healthPoints`: integer
- `spawnMapId`: string → map
- `primaryDropItemId`: string → item

Unique key: `monsterId`. Runtime rule: `monster.drop-weight-positive`.

## Capabilities

- `inspect-monster` via `graph`
- `clone-monster` via `graph`
- `tune-monster` via `graph`
- `edit-drop-table` via `text`
- `add-monster` via `graph`
- `replace-monster-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
