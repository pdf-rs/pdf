# object/types/page.rs — page objects, against PDF 32000-1:2008 §7.7.3.3

Reviewed: 2026-11-01.

## Mismatches

- **`inherit()` hangs on a `/Parent` cycle** (lines 187-200: an unbounded `loop` walking
  `parent.parent`). A malformed page tree whose `/Pages` nodes point at each other makes
  `media_box()`, `crop_box()`, `resources()` or `rotate()` loop forever — a trivial DoS on any
  consumer that walks pages. (`PageTree::page_limited` *does* cap depth at 16; the inheritance
  path does not.)
- **`/Rotate` is not normalised** (`rotate()`): the raw value is returned. 7.7.3.3 requires a
  multiple of 90; values such as 45 or 450 are passed on to geometry code unchecked, and
  negative values are not folded into 0-359.
- `/TrimBox`, `/BleedBox`, `/ArtBox` are parsed into fields but have no inheriting accessor
  (`media_box()`/`crop_box()` do); `/CropBox` correctly falls back to `/MediaBox` (7.7.3.3),
  which makes the asymmetry easy to trip over.
- `/UserUnit` is not parsed (7.7.3.3, added in PDF 1.6) — page size in user space is assumed to
  be 1/72 inch, so large-format pages report wrong physical dimensions.

## Checked, no mismatch

- Inheritable attribute walk order for `/Resources`, `/MediaBox`, `/CropBox`, `/Rotate`
  (nearest ancestor wins), `/CropBox` default = `/MediaBox`.
- `/Contents` as a single stream, an array, or absent (7.7.3.3 Table 30).
- `/Annots`, `/Metadata`, `/Group`, `/VP`, `/AA`, `/StructParents` parsed into the typed `Page`.
