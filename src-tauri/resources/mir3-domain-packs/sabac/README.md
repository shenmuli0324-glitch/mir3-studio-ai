# sabac

MIR3 Studio sabac domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `phaseId`: string
- `battleMapId`: string → map
- `startMinute`: integer
- `endMinute`: integer
- `guildRewardItemId`: string → item

Unique key: `phaseId`. Runtime rule: `sabac.phases-ordered-and-regions-contained`.

## Capabilities

- `inspect-sabac` via `map`
- `edit-sabac-phase` via `map`
- `edit-sabac-region` via `map`
- `validate-sabac-settlement` via `map`
- `add-sabac` via `map`
- `clone-sabac` via `map`
- `batch-update-sabac` via `map`
- `replace-sabac-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
