# cumulative_charge

MIR3 Studio cumulative_charge domain pack for MIR3 System Kernel v1. Pack version: `1.2.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `batch-update-cumulative_charge` via `timeline`
- `replace-cumulative_charge-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
