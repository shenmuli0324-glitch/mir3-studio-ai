# Development Tools Design QA

## Evidence

- Source visual truth:
  - `/Users/sml/Desktop/截屏2026-08-24 18.16.45.png` — development-tool catalog reference.
  - `/Users/sml/Desktop/截屏2026-08-24 18.21.07.png` — map-workspace reference.
- Browser-rendered implementation:
  - `docs/design-qa/devtools/catalog-implementation.png`
  - `docs/design-qa/devtools/map-implementation.png`
- Normalized side-by-side comparisons:
  - `docs/design-qa/devtools/catalog-comparison.png`
  - `docs/design-qa/devtools/map-comparison.png`
- CSS viewport: `1280 × 840`, device scale factor `1`.
- Source pixels: catalog `2884 × 1318`; map `2982 × 1548`.
- Implementation pixels: both `1280 × 840`.
- Comparison normalization: each source was scaled to `1024px` width and vertically padded to `1024 × 672`; each implementation was scaled to `1024 × 672`; pairs were joined into `2048 × 672` images.
- State: dark theme, Chinese locale, development-tool catalog and map system entry scaffold.

## Full-view comparison

The implementation preserves the reference hierarchy: persistent product navigation, a dark tool catalog with icon-led cards, and a map workspace divided into a secondary list, top action area, and central content area. The catalog intentionally uses four responsive columns and category sections because it contains 33 systems instead of the six tools shown by the reference. The map screen intentionally shows verified empty states instead of copying the reference's simulated map, NPC, and safe-zone data.

## Focused comparison

- Catalog card headers, status chips, titles, descriptions, and Gravity icons were inspected in the browser at their rendered size.
- Map secondary navigation, disabled action controls, and the content empty state were inspected in the browser at their rendered size.
- A separate crop was not required because all important UI text and controls remain legible in the normalized full-view comparisons; DOM accessibility snapshots were also used to verify labels and reading order.

## Required fidelity surfaces

- Fonts and typography: uses the existing MIR3 Studio font stack and type tokens; heading, description, category, card title, and metadata hierarchy are consistent with the current product shell.
- Spacing and layout rhythm: card gaps, section dividers, sidebar widths, toolbar height, and content centering follow the existing Studio spacing scale and remain usable at `1280 × 840`.
- Colors and tokens: all surfaces use existing `canvas`, `panel`, `panel2`, `line`, `ink`, `muted`, and `accent` tokens. No reference-specific hardcoded palette was introduced.
- Image and icon fidelity: there are no raster placeholders or hand-built SVGs. All system icons use the installed Gravity icon library; the existing MIR3 brand asset is reused.
- Copy and content: all 33 systems have synchronized Chinese and English titles and descriptions. Cross-server is explicitly numbered and ordered last.

## Interaction verification

- Opened the map system from the catalog.
- Returned from the map system to the catalog.
- Opened the NPC system and verified the shared planned-system workspace.
- Searched for `跨服` and verified that only system `33` remained.
- Verified 33 unique system entries and category labels through DOM snapshots and automated tests.
- Browser preview logs contain expected Tauri API errors because the dedicated visual-preview URL runs outside the Tauri host. The actual `pnpm tauri dev` run reached a healthy MIR3 AI Core response after startup and again after HMR; no development-tool rendering exception was reported by the desktop host.

## Comparison history

### Pass 1

- P2: disabled map toolbar and add-map controls had insufficient contrast because the default primary/disabled HeroUI treatment used a dark foreground.
- Fix: switched disabled actions to the ghost treatment with explicit existing `muted` and `line` tokens.

### Pass 2

- Post-fix evidence: `docs/design-qa/devtools/map-implementation.png` and `docs/design-qa/devtools/map-comparison.png`.
- No actionable P0, P1, or P2 visual issue remains for the approved entry-and-scaffold scope.

## Follow-up polish

- P3: when real map data is connected, tune list-row density and add the selected-map inspector against representative 996 project fixtures.
- P3: verify the catalog at the minimum supported desktop window size with real localized strings after future system names are finalized.

final result: passed
