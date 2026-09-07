# quest

MIR3 Studio quest domain pack for MIR3 System Kernel v1. Pack version: `1.3.2`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access requires the real project layout, an evidence-backed file binding, and resource-schema validation; keywords never grant ownership. unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `questId`: string
- `startNpcId`: string → npc
- `targetMonsterId`: string → monster
- `rewardItemId`: string → item
- `minimumLevel`: integer
- `nextQuestId`: string

Unique key: `questId`. Runtime rule: `quest.chain-acyclic-and-reachable`.

## Capabilities

- `inspect-quest` via `graph`
- `clone-quest-chain` via `graph`
- `insert-quest-step` via `graph`
- `replace-quest-reward` via `graph`
- `batch-update-quest` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
