# limited_event

MIR3 Studio limited_event domain pack for MIR3 System Kernel v1. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `eventId`: string
- `startEpochSeconds`: integer
- `endEpochSeconds`: integer
- `eventMapId`: string → map
- `questId`: string → quest

Unique key: `eventId`. Runtime rule: `limited-event.start-before-end`.

## Capabilities

- `inspect-limited-event` via `timeline`
- `clone-limited-event` via `timeline`
- `shift-event-window` via `timeline`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
