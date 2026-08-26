# skill

MIR3 Studio skill domain pack for MIR3 System Kernel v1. Pack version: `1.3.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `>=1.0.0`. Engine versions are normalized only from SemVer, v-prefixed SemVer, or major.minor aliases. Write access additionally requires the real project layout, an owned selector or content fingerprint, and resource-schema validation; unknown/incompatible engines and unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `skillId`: string
- `skillLevel`: integer
- `manaCost`: integer
- `cooldownMilliseconds`: integer
- `appliedBuffId`: string → buff

Unique key: `skillId + skillLevel`. Runtime rule: `skill.level-curve-contiguous`.

## Capabilities

- `inspect-skill` via `graph`
- `clone-skill` via `graph`
- `generate-skill-curve` via `graph`
- `bind-skill-effect` via `graph`
- `batch-update-skill` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
