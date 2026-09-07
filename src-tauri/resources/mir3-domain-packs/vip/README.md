# vip

MIR3 Studio vip domain pack for MIR3 System Kernel v1. Pack version: `1.3.2`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access requires the real project layout, an evidence-backed file binding, and resource-schema validation; keywords never grant ownership. unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `clone-vip` via `xls`
- `replace-vip-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
