# ranking

MIR3 Studio ranking domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `boardId`: string
- `metric`: string
- `cycleSeconds`: integer
- `rewardItemId`: string → item
- `seasonId`: string → season

Unique key: `boardId`. Runtime rule: `ranking.settlement-within-cycle`.

## Capabilities

- `inspect-ranking` via `xls`
- `clone-ranking` via `xls`
- `edit-ranking-cycle` via `xls`
- `replace-ranking-reward` via `xls`
- `add-ranking` via `xls`
- `batch-update-ranking` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
