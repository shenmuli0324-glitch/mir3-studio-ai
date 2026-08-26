# resource_production

MIR3 Studio resource_production domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
