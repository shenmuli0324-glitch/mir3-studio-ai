# first_charge

MIR3 Studio first_charge domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `add-first_charge` via `graph`
- `batch-update-first_charge` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
