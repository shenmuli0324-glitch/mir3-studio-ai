# manor

MIR3 Studio manor domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
