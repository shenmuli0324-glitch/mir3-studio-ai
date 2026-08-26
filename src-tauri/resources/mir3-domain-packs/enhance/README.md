# enhance

MIR3 Studio enhance domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `enhanceTier`: integer
- `equipmentClass`: string
- `successRateBasisPoints`: integer
- `materialItemId`: string → item
- `failureMode`: string

Unique key: `enhanceTier + equipmentClass`. Runtime rule: `enhance.probability-budget-valid`.

## Capabilities

- `inspect-enhancement` via `xls`
- `generate-enhancement-tiers` via `xls`
- `tune-enhancement-probability` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
