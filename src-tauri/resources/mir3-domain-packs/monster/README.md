# monster

MIR3 Studio monster domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
