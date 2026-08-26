# shop

MIR3 Studio shop domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
