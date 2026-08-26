# Changelog

## 1.2.0 - 2026-08-26

- Validate the complete Kernel API v1 manifest contract, including schema versions, primitives, projections, resources, presentations, operations, validators, and runtime fixture paths.
- Added a runtime-installable example pack and a shared accepted/rejected SDK-to-Rust contract corpus.

## 1.1.0 - 2026-08-26

- Require explicit non-wildcard engine ranges and evidence-gated version aliases.
- Require unknown and incompatible engine versions to fail read-only.

## 1.0.0 - 2026-08-26

- Added public declarative manifest and fixture entrypoints for Kernel API v1.
- Bound generated plugins to registered renderers, safe runtime primitives, closed schemas, and Draft safety gates.
- Rejected arbitrary shell, executable module, component, and code payloads.
