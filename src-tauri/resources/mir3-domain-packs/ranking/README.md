# ranking

MIR3 Studio ranking domain pack for MIR3 System Kernel v1. Pack version: `1.3.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
