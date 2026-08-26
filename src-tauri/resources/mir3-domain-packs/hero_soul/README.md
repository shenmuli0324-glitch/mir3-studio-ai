# hero_soul

MIR3 Studio hero_soul domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `routeId`: string
- `nodeId`: string
- `costItemId`: string → item
- `grantedSkillId`: string → skill
- `powerValue`: integer

Unique key: `routeId + nodeId`. Runtime rule: `hero-soul.route-acyclic-and-affordable`.

## Capabilities

- `inspect-hero-soul` via `graph`
- `add-hero-soul-route` via `graph`
- `batch-edit-hero-soul` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
