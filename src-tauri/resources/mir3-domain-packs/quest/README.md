# quest

MIR3 Studio quest domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
