# map

MIR3 Studio map domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
