# npc

MIR3 Studio npc domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `add-npc` via `graph`
- `clone-npc` via `graph`
- `batch-update-npc` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
