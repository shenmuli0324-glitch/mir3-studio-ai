# first_charge

MIR3 Studio first_charge domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `tierId`: string
- `chargeThreshold`: number
- `rewardItemId`: string → item
- `rewardCount`: integer
- `minimumVipLevel`: integer

Unique key: `tierId`. Runtime rule: `first-charge.first-tier-is-minimum`.

## Capabilities

- `inspect-first-charge` via `graph`
- `replace-first-charge-reward` via `graph`
- `clone-first-charge-tier` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
