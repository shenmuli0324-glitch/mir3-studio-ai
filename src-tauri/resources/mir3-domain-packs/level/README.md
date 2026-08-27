# level

MIR3 Studio level domain pack for MIR3 System Kernel v1. Pack version: `1.3.1`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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
- `add-level` via `xls`
- `clone-level` via `xls`
- `replace-level-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
