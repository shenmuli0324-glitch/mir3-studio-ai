# cross_server

MIR3 Studio cross_server domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
