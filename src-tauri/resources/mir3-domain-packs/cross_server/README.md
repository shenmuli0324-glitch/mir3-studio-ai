# cross_server

MIR3 Studio cross_server domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `routeId`: string
- `sourceShard`: string
- `targetShard`: string
- `minimumEngineVersion`: string
- `maximumEngineVersion`: string
- `maximumConcurrentPlayers`: integer
- `seasonId`: string → season

Unique key: `routeId`. Runtime rule: `cross-server.route-and-engine-range-compatible`.

## Capabilities

- `inspect-cross-server` via `graph`
- `edit-cross-server-route` via `graph`
- `validate-cross-server-compatibility` via `graph`
- `add-cross_server` via `graph`
- `clone-cross_server` via `graph`
- `batch-update-cross_server` via `graph`
- `replace-cross_server-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
