# title

MIR3 Studio title domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `titleId`: string
- `displayLabel`: string
- `grantedBuffId`: string → buff
- `durationSeconds`: integer
- `minimumLevel`: integer

Unique key: `titleId`. Runtime rule: `title.permanent-duration-zero`.

## Capabilities

- `inspect-title` via `xls`
- `clone-title` via `xls`
- `batch-edit-title` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
