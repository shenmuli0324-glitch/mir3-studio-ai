# crafting

MIR3 Studio crafting domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `recipeId`: string
- `outputItemId`: string → item
- `outputCount`: integer
- `materialItemId`: string → item
- `materialCount`: integer

Unique key: `recipeId`. Runtime rule: `crafting.no-self-consuming-cycle`.

## Capabilities

- `inspect-recipe` via `graph`
- `clone-recipe` via `graph`
- `replace-recipe-material` via `graph`
- `scale-recipe` via `graph`
- `add-crafting` via `graph`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
