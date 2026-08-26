# cumulative_charge

MIR3 Studio cumulative_charge domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `tierId`: string
- `cycleId`: string
- `chargeThreshold`: number
- `rewardItemId`: string → item
- `rewardCount`: integer

Unique key: `cycleId + chargeThreshold`. Runtime rule: `cumulative-charge.thresholds-strictly-increase`.

## Capabilities

- `inspect-cumulative-charge` via `timeline`
- `generate-charge-tiers` via `timeline`
- `clone-charge-cycle` via `timeline`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
