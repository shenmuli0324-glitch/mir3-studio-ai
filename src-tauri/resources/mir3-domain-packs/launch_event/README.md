# launch_event

MIR3 Studio launch_event domain pack for MIR3 System Kernel v1. Pack version: `1.1.0`; compiler compatibility: MIR3 System Kernel `^1.0.0`; engine range: `*`. Unknown formats are always read-only. Mutations use registered safe primitives and external drafts.

## Resource schema

- `scheduleId`: string
- `openServerDay`: integer
- `eventId`: string → limited_event
- `rewardItemId`: string → item
- `rewardCount`: integer

Unique key: `scheduleId`. Runtime rule: `launch-event.day-windows-nonoverlapping`.

## Capabilities

- `inspect-launch-event` via `timeline`
- `clone-launch-event` via `timeline`
- `shift-launch-schedule` via `timeline`
- `add-launch_event` via `timeline`
- `batch-update-launch_event` via `timeline`
- `replace-launch_event-reference` via `text`

## Contract fixtures

The `fixtures/valid.json` and `fixtures/invalid.json` corpora are checked against `schemas/resource.schema.json`; expected validator output is in `fixtures/expected-diagnostics.json`.
