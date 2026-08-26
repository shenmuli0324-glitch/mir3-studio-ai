# rebirth

MIR3 Studio rebirth domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `clone-rebirth` via `graph`
- `replace-rebirth-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
