# level

MIR3 Studio level domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `level`: integer
- `requiredExperience`: integer
- `statPoints`: integer
- `recommendedMonsterId`: string → monster

Unique key: `level`. Runtime rule: `level.experience-monotonic`.

## Capabilities

- `inspect-level-curve` via `xls`
- `scale-experience` via `xls`
- `interpolate-levels` via `xls`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
