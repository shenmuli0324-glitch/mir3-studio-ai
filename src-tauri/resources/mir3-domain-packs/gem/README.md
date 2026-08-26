# gem

MIR3 Studio gem domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `gemId`: string
- `gemTier`: integer
- `socketType`: string
- `itemId`: string → item
- `grantedBuffId`: string → buff

Unique key: `gemId + gemTier`. Runtime rule: `gem.tier-chain-contiguous`.

## Capabilities

- `inspect-gem` via `graph`
- `generate-gem-tiers` via `graph`
- `edit-gem-slot` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
