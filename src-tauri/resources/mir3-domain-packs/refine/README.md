# refine

MIR3 Studio refine domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `poolId`: string
- `equipmentId`: string → equipment
- `attributeKey`: string
- `weight`: integer
- `minimumValue`: number
- `maximumValue`: number

Unique key: `poolId + equipmentId + attributeKey`. Runtime rule: `refine.minimum-not-greater-than-maximum`.

## Capabilities

- `inspect-refine-pool` via `xls`
- `edit-refine-weight` via `xls`
- `clone-refine-template` via `xls`
- `add-refine` via `xls`
- `batch-update-refine` via `xls`
- `replace-refine-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
