# recycle

MIR3 Studio recycle domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `ruleId`: string
- `itemType`: string
- `minimumQuality`: integer
- `maximumQuality`: integer
- `currencyItemId`: string → item
- `returnValue`: integer

Unique key: `ruleId`. Runtime rule: `recycle.quality-range-ordered`.

## Capabilities

- `inspect-recycle` via `xls`
- `batch-edit-recycle` via `xls`
- `preview-recycle-value` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
