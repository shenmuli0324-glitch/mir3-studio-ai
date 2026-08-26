# sabac

MIR3 Studio sabac domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
