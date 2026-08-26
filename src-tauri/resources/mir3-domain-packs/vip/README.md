# vip

MIR3 Studio vip domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `vipLevel`: integer
- `requiredPoints`: integer
- `shopDiscountBasisPoints`: integer
- `grantedTitleId`: string → title

Unique key: `vipLevel`. Runtime rule: `vip.points-monotonic`.

## Capabilities

- `inspect-vip` via `xls`
- `generate-vip-tiers` via `xls`
- `batch-edit-vip-benefits` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
