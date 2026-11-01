# object/stream.rs — streams and object streams, against PDF 32000-1:2008 §7.3.8, §7.5.7

Reviewed: 2026-11-01.

## Mismatches

- **Object-stream header object numbers are ignored** (`ObjectStream::from_primitive`). Only the
  offsets are kept; the `obj_nr offset` pairs' first element is parsed and discarded. 7.5.7: the
  index from the xref stream addresses the *N*-th pair, which is what the crate does, so lookup
  works — but a stream whose header numbers are inconsistent with the referring xref type-2 entry
  is not detected (other readers return the object the xref names, this one returns position N).
- **`/First` and the offsets are not validated**: offsets are not required to be ascending, are
  not checked against `/First`, and only `/N * 2 <= data.len()` is enforced (hardening added in
  the earlier audit). `get_object_slice` computes `first + offset`, which can land past the end
  or overlap the header; the slice then either fails in the lexer or yields a partial object.
  A `/First` beyond the decoded length gives `start > data.len()` and a panic in
  `Lexer::new(&data[..])` style slicing on some paths.
- **A stream nested inside an object stream is not rejected** — 7.5.7: "an object stream shall
  not contain stream objects"; such an object is to be treated as null, here it parses into a
  `Stream` with a bogus `/Length` or an error.
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
