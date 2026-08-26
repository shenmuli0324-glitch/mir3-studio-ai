# season

MIR3 Studio season domain pack for MIR3 System Kernel v1. Pack version: `1.3.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `add-season` via `timeline`
- `batch-update-season` via `timeline`
- `replace-season-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
