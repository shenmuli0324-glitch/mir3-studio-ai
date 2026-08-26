# online_reward

MIR3 Studio online_reward domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `rewardId`: string
- `durationSeconds`: integer
- `rewardItemId`: string → item
- `rewardCount`: integer
- `minimumVipLevel`: integer

Unique key: `rewardId`. Runtime rule: `online-reward.duration-monotonic`.

## Capabilities

- `inspect-online-reward` via `timeline`
- `edit-online-duration` via `timeline`
- `replace-online-reward` via `timeline`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
