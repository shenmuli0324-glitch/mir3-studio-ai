# skill

MIR3 Studio skill domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

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

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
