# resource_production

MIR3 Studio resource_production domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `pointId`: string
- `mapId`: string → map
- `outputItemId`: string → item
- `intervalSeconds`: integer
- `yieldCount`: integer
- `guardMonsterId`: string → monster

Unique key: `pointId`. Runtime rule: `production.point-inside-map-and-rate-positive`.

## Capabilities

- `inspect-production` via `graph`
- `edit-production-rate` via `graph`
- `clone-production-point` via `graph`
- `add-resource_production` via `graph`
- `batch-update-resource_production` via `graph`
- `replace-resource_production-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
