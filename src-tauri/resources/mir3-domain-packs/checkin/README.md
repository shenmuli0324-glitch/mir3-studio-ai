# checkin

MIR3 Studio checkin domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `cycleId`: string
- `dayIndex`: integer
- `rewardItemId`: string → item
- `rewardCount`: integer
- `vipMultiplier`: number

Unique key: `cycleId + dayIndex`. Runtime rule: `checkin.days-contiguous`.

## Capabilities

- `inspect-checkin` via `timeline`
- `fill-checkin-rewards` via `timeline`
- `clone-checkin-cycle` via `timeline`
- `batch-update-checkin` via `timeline`
- `replace-checkin-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
