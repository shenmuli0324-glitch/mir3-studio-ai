# npc

MIR3 Studio npc domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `npcId`: string
- `scriptPath`: string
- `mapId`: string → map
- `coordinateX`: integer
- `coordinateY`: integer
- `shopId`: string → shop

Unique key: `npcId`. Runtime rule: `npc.script-entry-resolves`.

## Capabilities

- `inspect-npc` via `graph`
- `move-npc` via `graph`
- `edit-dialogue` via `text`
- `replace-npc-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
