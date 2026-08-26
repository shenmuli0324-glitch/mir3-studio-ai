# season

MIR3 Studio season domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `seasonId`: string
- `startEpochSeconds`: integer
- `endEpochSeconds`: integer
- `rankingBoardId`: string → ranking
- `seasonShopId`: string → shop

Unique key: `seasonId`. Runtime rule: `season.window-and-settlement-ordered`.

## Capabilities

- `inspect-season` via `timeline`
- `clone-season` via `timeline`
- `shift-season` via `timeline`
- `validate-season-settlement` via `timeline`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
