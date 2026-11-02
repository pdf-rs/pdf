# parser/parse_xref.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status: **fixed** (commit "spec: fix wave 3") — a reserved entry type is now an
`XRef::Invalid` entry instead of an error for the whole file; case `c15_*`
(a file whose xref stream carries a type-3 entry) fails on master and renders
"Hello" in mupdf as well.

## Mismatches

- **xref stream entry type > 2 is a hard error** (~line 44). 7.5.8.3 Table 17: "any other value
  shall be treated as a reference to the null object". Tolerant behaviour is required here;
  the crate rejects the whole file instead of making the object null.
- Classic table subsections: entry count is now bounded by remaining input (commit f0f1ded of
  the earlier audit), which is a deviation in the safe direction — no spec conflict.

## Checked, no mismatch

- `xref` keyword, subsection header `first count`, fixed 20-byte entries (`nnnnnnnnnn ggggg n`),
  `trailer << … >>` termination, `/Prev` chains (the chain itself is walked by `file.rs`).

## Related

- `/XRefStm` (hybrid-reference files, 7.5.8.4) is not read anywhere in the crate — see
  `../../xref.rs.md`.
