# equipment

MIR3 Studio equipment domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `equipmentId`: string
- `slot`: string
- `baseItemId`: string → item
- `requiredLevel`: integer
- `durability`: integer

Unique key: `equipmentId`. Runtime rule: `equipment.slot-matches-item-mode`.

## Capabilities

- `inspect-equipment` via `xls`
- `clone-equipment` via `xls`
- `batch-tune-equipment` via `xls`
- `replace-equipment-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
