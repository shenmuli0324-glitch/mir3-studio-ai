# talent

MIR3 Studio talent domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `nodeId`: string
- `treeId`: string
- `costPoints`: integer
- `requiredLevel`: integer
- `grantedSkillId`: string → skill
- `parentNodeId`: string

Unique key: `treeId + nodeId`. Runtime rule: `talent.graph-acyclic-and-budget-valid`.

## Capabilities

- `inspect-talent` via `graph`
- `edit-talent-node` via `graph`
- `edit-talent-edge` via `graph`
- `validate-talent-budget` via `graph`
- `add-talent` via `graph`
- `clone-talent` via `graph`
- `batch-update-talent` via `graph`
- `replace-talent-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
