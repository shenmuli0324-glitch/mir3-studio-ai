# Changelog

## 1.2.0

- Replaced the wildcard engine declaration with evidence-gated automatic generalization for recognized SemVer aliases.
- Made unknown and incompatible engine versions explicitly read-only before Draft writes and final Apply.

## 1.1.0

- Completed the registered create, clone, batch-update, and reference-replacement operation families with closed parameter schemas and Draft safety gates.
- Kept all writes scoped to this domain and compiled only through registered safe primitives.

## 1.0.0

- Added the manor-record resource schema with typed fields, unique keys, references, client/engine consistency, and runtime diagnostics.
- Added parameterized safe operations backed by the map primitive.
- Added valid and invalid contract fixtures with expected diagnostics.
