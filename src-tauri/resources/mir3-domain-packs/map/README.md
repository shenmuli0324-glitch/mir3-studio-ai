# map

MIR3 Studio map domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `mapId`: string
- `displayName`: string
- `width`: integer
- `height`: integer
- `safeZoneMode`: string
- `spawnNpcId`: string → npc

Unique key: `mapId`. Runtime rule: `map.bounds-contain-spawns`.

## Capabilities

- `inspect-map` via `map`
- `clone-map` via `map`
- `edit-map-config` via `text`
- `edit-map-region` via `map`
- `add-map` via `map`
- `batch-update-map` via `map`
- `replace-map-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
