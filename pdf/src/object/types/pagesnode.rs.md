# object/types/pagesnode.rs — page tree, against PDF 32000-1:2008 §7.7.3.2

Reviewed: 2026-11-01.

## Mismatches

- **`/Count` is trusted, never verified** (`PageTree::page_limited`). 7.7.3.2: `/Count` is "the
  number of leaf nodes … in the subtree", i.e. a hint the reader may rely on but which must agree
  with the tree. A wrong `/Count` silently skips or duplicates pages: with `Count` too small,
  `page_nr` falls through into the *next* sibling and returns the wrong page; too large and
  requests land on `PageOutOfBounds`.
- **`/Kids` may be missing or of the wrong kind** — `kids: Vec<Ref<PagesNode>>` defaults to empty
  and there is no `/Kids` requirement (Table 29 makes it required and non-empty); an empty tree
  reports every request as out of bounds rather than as a malformed document.
- Cycle protection exists only via the depth counter (16) in `page_limited`; a cycle *shorter*
  than 16 levels yields a wrong page number instead of an error, and the sibling walk can visit
  the same node repeatedly.
- `/Type` is required to be exactly `Page` or `Pages`; a missing `/Type` on a `/Pages` node is an
  error, while 7.7.3.2 makes `/Type` required — so this is spec-conforming but less tolerant than
  most readers (noted, not a violation).

## Checked, no mismatch

- Page-tree node polymorphism (`/Type /Page` vs `/Type /Pages`), `/Parent` field, `/Kids` order =
  page order (7.7.3.2), `/Count` arithmetic per subtree for the descent.
