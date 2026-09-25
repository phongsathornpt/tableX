# Design QA — Option 2 workspace

## Evidence and setup

- Source visual truth: `/home/gg/.codex/generated_images/01a0d43f-55c6-7ac3-a8a3-ea7ecaf4ae05/exec-34627849-cb72-4212-a6c6-c5a71681673a.png`
- Implementation screenshot: `/tmp/tablex-design/implementation-final.png`
- Full-view comparison: `/tmp/tablex-design/full-comparison-final.png`
- Focused grid comparison: `/tmp/tablex-design/focused-grid-comparison.png`
- Focused object-list comparison: `/tmp/tablex-design/table-list-comparison.png`
- Viewport: 1487 × 1058 px; source and implementation images are both 1487 × 1058 px.
- CSS size: not applicable; native GPUI window. Captured at 1:1, 1× density in Xvfb.
- State: connected to an isolated local PostgreSQL 17.10 fixture; `public.admin_users` selected; 6 columns, 25 loaded rows; Tables group expanded, Views collapsed; no row filters active; SQL dock open.
- The mock includes desktop window controls, while the Xvfb capture has no window manager decorations. The app content itself was compared at equal pixel scale; this frame difference is not counted as product drift.

## Findings

No actionable P0, P1, or P2 visual mismatches remain in the final full-view and focused comparisons.

## Open questions

- The reference includes Functions, Types, and History navigation. The current product data model exposes table/view objects and has no query-history screen. These entries were not fabricated for visual parity. If those capabilities are added, expose them as real navigable groups/items.
- The fixture reports PostgreSQL 17.10, whereas the mock displays PostgreSQL 18.3; the version label is live connection context, not a design token.

## Implementation checklist

- [x] Match the selected-table workspace hierarchy, dark navy/teal palette, explorer rail, object groups, data canvas, and bottom SQL dock.
- [x] Keep the Tables group expanded and make object groups collapsible; default Views to collapsed.
- [x] Use bounded, horizontally scrollable table rows with typed headers, row striping, Boolean state chips, monospace data values, and hover access to long cell values.
- [x] Add clear dropdown affordances for filter scope, value filters, sort, page size, and visible columns; keep row search at the reference scale.
- [x] Remove the redundant selected-table Columns card so the object explorer keeps its full height.
- [x] Verify the Views group expands and collapses in the running UI.
- [x] Left-align explorer rows, use disclosure/type icons, highlight the selected table in teal, and consolidate search plus schema/type filters into the full-width object-search field.

## Required fidelity surfaces

- Fonts and typography: UI labels and headings use the existing GPUI theme; data values and row indices use its monospace face, as in the reference’s dense data grid. Headers remain readable at the captured size; long cell values stay on one line and are available through tooltips rather than wrapping into uneven rows.
- Spacing and layout rhythm: rail and explorer proportions, grid toolbar, data rows, and bottom query dock follow the reference composition. The redundant sidebar details block was removed. The only capture-level offset is the absent Xvfb window frame noted above.
- Colors and visual tokens: the implementation uses the selected dark navy surfaces, subtle blue borders/alternating rows, teal primary/active states, and muted secondary text. Contrast and semantic status colors remain consistent across the screen.
- Image quality and asset fidelity: no raster artwork is present in the source. UI icons use the project’s standard vector icon assets; no placeholder or hand-drawn artwork was introduced.
- Copy and content: table/schema names, column types, row counts, filter actions, and query controls are live UI labels/data. Version and fixture values differ from the mock because they come from the local PostgreSQL fixture. Filter menus and row actions retain explicit names.

## Comparison history

1. Initial selected-table capture showed the data body and metadata region collapsing to zero height (P1: core inspection content was missing). The scroll/metadata containers were given full-size and minimum-height constraints. `/tmp/tablex-design/selected-table-v3.png` confirmed visible rows and column data; the final capture confirms the layout remains stable.
2. The focused grid comparison exposed an oversized search field, dropdowns without visible menu affordances, and sans-serif values that weakened scanability. The toolbar was rebalanced, the native dropdown caret and column icon were added, and grid values were switched to the theme monospace face. The post-fix evidence is `/tmp/tablex-design/focused-grid-comparison.png`.
3. The full-view comparison exposed an always-expanded Views group and a selected-table Columns card that reduced explorer browsing height (P2 composition drift). Category groups are now interactive, Views defaults collapsed, and the redundant card was removed. Expansion was exercised in the running app; final full-view evidence is `/tmp/tablex-design/full-comparison-final.png`.
4. A render-time reentrant entity read caused the app to panic during an earlier capture. Render state is now passed as owned values through the view boundary; the rebuilt app launched, connected to the fixture, and rendered the selected-table state without that panic.
5. The follow-up object-list comparison exposed a real row-component miss: full-width Buttons centered their labels, the active table was gray, and an empty filter-chip row added vertical whitespace (P2 list fidelity). Replaced schema/category/table rows with the kit ListItem, restored the intended indentation and teal selected treatment, removed the empty chip row, and placed Search and schema/type filtering inside the full-width object-search field. The final list and full comparisons are `/tmp/tablex-design/table-list-comparison.png` and `/tmp/tablex-design/full-comparison-final.png`. Manual checks confirmed the filter menu opens and searching `audit` narrows the list to the matching table.

## Follow-up polish (P3)

- Add per-column sort indicators/header sorting if desired; sorting is currently available through the visible Sort menu.
- Add matching History, Functions, and Types entries only when those product capabilities return real data and have working destinations.
- The row SQL ellipsis actions are retained for the existing reviewable SQL workflow although they are not shown in the mock.
- The mock’s remaining table names are ordered differently; the live explorer keeps the provider’s alphabetical order for predictable browsing.

## Verification

- `make verify` with `RUSTUP_TOOLCHAIN=1.96.1`: passed (format check, `cargo check`, Clippy with warnings denied, and tests: 20 passed, 1 ignored opt-in PostgreSQL test).
- `cargo build` with `RUSTUP_TOOLCHAIN=1.96.1`: passed.
- Manual native UI check: passed against an isolated local PostgreSQL fixture; table selection, loaded rows, Views group collapse/expand, and final layout were observed. This is not a production-server check.
- `git diff --check`: passed.

final result: passed
