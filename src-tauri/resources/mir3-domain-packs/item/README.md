# item

MIR3 Studio item domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `itemId`: string
- `itemType`: string
- `stackLimit`: integer
- `clientIcon`: string
- `engineStdMode`: integer
- `linkedBuffId`: string → buff

Unique key: `itemId`. Runtime rule: `item.icon-resource-exists`.

## Capabilities

- `inspect-item` via `xls`
- `clone-item` via `xls`
- `batch-edit-item` via `xls`
- `replace-item-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
