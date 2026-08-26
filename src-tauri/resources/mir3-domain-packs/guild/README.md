# guild

MIR3 Studio guild domain pack for MIR3 System Kernel v1. Pack version: `1.3.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `clone-guild` via `graph`
- `batch-update-guild` via `graph`
- `replace-guild-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
