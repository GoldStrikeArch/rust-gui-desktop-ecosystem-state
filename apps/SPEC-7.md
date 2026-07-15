# SPEC-7: "Grid" — 100k-row data table test

Iteration 4. The data-dense business-app dimension: virtualization, sorting,
filtering, and custom cells at scale.

## Functional requirements

1. **Window** titled `Grid (<framework>)`, ~1000×640.
2. **Data**: 100,000 rows generated deterministically in code (seeded PRNG —
   same data every run): `id` (u32), `name` (e.g. "adjective-noun-####"),
   `category` (one of 8), `value` (f64, 2 decimals, right-aligned), `date`
   (ISO yyyy-mm-dd), `status` (Ok/Warn/Err) rendered as a **colored chip**
   (custom cell rendering test).
3. **Virtualized table** — scrolling must not degrade at 100k rows.
4. **Sort**: click a column header to sort asc, click again for desc, with a
   visible sort indicator.
5. **Filter-as-you-type**: a text input filters rows by substring on `name`;
   show "N of 100,000 rows". **Self-time each filter application and print
   `FILTER_MS <query_len> <ms>` to stdout** (retained as evidence).
6. **Row selection**: click selects (highlight); Shift-click range selection
   if the framework affords it (documented approximation otherwise).
7. **Column resize**: drag a header divider to resize a column (documented
   approximation OK — e.g. resize buttons — this is expected to be the
   weakest cell).
8. Also print `BUILD_MS <ms>` (data generation + initial model build) once at
   startup.

## FRICTION.md (required — audit conventions)

Per capability: rating + evidence label + note:
table_widget (what exists in this framework/ecosystem and what you used),
virtualization, sort, filter_latency (typical FILTER_MS at 1-char and 4-char
queries), column_resize, row_selection, cell_custom_render.
Also: helper crates + why; LoC split production/verification; MiB units;
where the time went; RSS after load + after a long scroll (self-observed via
ps, note the command).

## Implementation rules

Independent crate `apps/<framework>-grid/` (package `<framework>-grid`), same
pinned framework version as `apps/<framework>-app/`, fallback rule, build +
launch verification with evidence labels. Shared-desktop rules as SPEC-6.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
