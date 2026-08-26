# guild

MIR3 Studio guild domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `guildLevel`: integer
- `requiredContribution`: integer
- `maximumMembers`: integer
- `rankingBoardId`: string → ranking

Unique key: `guildLevel`. Runtime rule: `guild.members-and-contribution-monotonic`.

## Capabilities

- `inspect-guild` via `graph`
- `edit-guild-permission` via `graph`
- `generate-guild-levels` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
