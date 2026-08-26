# rebirth

MIR3 Studio rebirth domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `rebirthTier`: integer
- `minimumLevel`: integer
- `costItemId`: string → item
- `costAmount`: integer
- `grantedTitleId`: string → title

Unique key: `rebirthTier`. Runtime rule: `rebirth.minimum-level-reachable`.

## Capabilities

- `inspect-rebirth` via `graph`
- `add-rebirth-tier` via `graph`
- `batch-edit-rebirth` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
