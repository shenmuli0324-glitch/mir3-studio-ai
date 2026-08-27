# resource_production

MIR3 Studio resource_production domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
