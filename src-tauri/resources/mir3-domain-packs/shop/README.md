# shop

MIR3 Studio shop domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `offerId`: string
- `shopId`: string
- `itemId`: string → item
- `currencyItemId`: string → item
- `price`: number
- `startEpochSeconds`: integer
- `endEpochSeconds`: integer

Unique key: `offerId`. Runtime rule: `shop.sale-window-and-price-valid`.

## Capabilities

- `inspect-shop` via `xls`
- `batch-price-shop` via `xls`
- `schedule-shop-item` via `xls`
- `replace-shop-item` via `xls`
- `add-shop` via `xls`
- `clone-shop` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
