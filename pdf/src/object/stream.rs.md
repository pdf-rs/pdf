# object/stream.rs — streams and object streams, against PDF 32000-1:2008 §7.3.8, §7.5.7

Reviewed: 2026-11-01.

## Mismatches

- **Object-stream header object numbers are ignored** (`ObjectStream::from_primitive`). Only the
  offsets are kept; the `obj_nr offset` pairs' first element is parsed and discarded. 7.5.7: the
  index from the xref stream addresses the *N*-th pair, which is what the crate does, so lookup
  works — but a stream whose header numbers are inconsistent with the referring xref type-2 entry
  is not detected (other readers return the object the xref names, this one returns position N).
- **`/First` and the offsets are only partially validated**: `/N * 2 <= data.len()` bounds the
  header (hardening added in the earlier audit), and the range of each entry is now checked
  against the decoded length before it is handed out, so a bad offset is an error instead of a
  slice panic. Offsets are still not required to be ascending, and a range that overlaps the
  header (before `/First`) is still accepted.
- **A stream nested inside an object stream is not rejected** — 7.5.7: "an object stream shall
  not contain stream objects"; such an object is to be treated as null, here it parses into a
  `Stream` with a bogus `/Length` or an error.
- ~~**`/DecodeParms` dropped / misplaced on write**~~ — **fixed**. `to_pdf_stream` collected a
  single dictionary in a loop over the filters and `assert!`ed that no second filter had
  parameters: a chain with two parameterised filters panicked on save, and with one it wrote
  that dictionary for filter 0 whatever the filter order was. `/DecodeParms` is now built per
  filter, `null` for the ones without parameters, as an array parallel to `/Filter` (7.4.2
  Table 5); a lone filter keeps the direct dictionary form. Test:
  `regressions::decode_parms_stay_aligned_with_the_filters`.
- ~~**`get_object_slice` can slice out of bounds**~~ — **fixed**. `/First` plus an entry offset
  can point past the decoded data or backwards; the range is validated before it is returned.
- **`/FFilter` and `/FDecodeParms` length mismatch is silently tolerated** (missing params default
  to an empty dictionary instead of the pair being invalid), 7.4.2 Table 5 — tolerant, but it
  hides corrupt external-file definitions.

## Checked, no mismatch

- `StreamInfo::from_primitive`: `/Length` required (7.3.8.2), `/Filter` as name *or* array,
  `/DecodeParms` as null / dict / array with null holes, `/F`, `/FFilter`, `/FDecodeParms`
  (7.4.2 Tables 5, 7), and the filter/params pairing by index.
- Filter names → `StreamFilter` mapping for all 10 filters of Table 6 (unknown names error out).
- Writing: `/Filter` written as a name when there is one filter, as an array otherwise;
  `/DecodeParms` written per filter, including explicit nulls (7.3.8.2).
- Decoded-data and stream caches are keyed per object; `get_data_or_decode` returns the cached
  decoded bytes (no spec relevance, noted for agents).
- `ObjStm /N` is bounded by half the decoded byte count before allocation (hardening).
