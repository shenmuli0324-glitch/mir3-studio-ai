# recycle

MIR3 Studio recycle domain pack for MIR3 System Kernel v1. Pack version: `1.3.2`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access requires the real project layout, an evidence-backed file binding, and resource-schema validation; keywords never grant ownership. unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `add-recycle` via `xls`
- `clone-recycle` via `xls`
- `replace-recycle-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
