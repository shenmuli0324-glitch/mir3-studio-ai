# gem

MIR3 Studio gem domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `clone-gem` via `graph`
- `batch-update-gem` via `graph`
- `replace-gem-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
