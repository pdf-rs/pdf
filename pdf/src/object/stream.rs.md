# object/stream.rs — streams and object streams, against PDF 32000-1:2008 §7.3.8, §7.5.7

Reviewed: 2026-11-01.

## Mismatches

- ~~**Object-stream header object numbers are ignored**~~ — **fixed**.
  `ObjectStream::from_primitive` used to keep only the offsets and throw the `obj_nr offset`
  pairs' first element away. 7.5.7 repeats the object number next to each offset precisely so a
  reader can check that the xref type 2 entry and the stream agree. The numbers are now kept and
  exposed (`object_nr`, `index_of`); `Storage::resolve_ref` prefers the xref index, falls back to
  the header if the two disagree, and errors when the stream does not hold the object at all.
  Test: `object::stream::tests::object_stream_keeps_the_header_numbers`.
- ~~**`/First` and the offsets are only partially validated**~~ — **fixed**. `/N * 2 <= data.len()`
  bounds the header, decreasing offsets are rejected (7.5.7 stores the objects one after the
  other, so a backwards offset makes the previous entry's range negative), and the range of each
  entry is checked against the decoded length before it is handed out.
  Test: `object_stream_rejects_backwards_offsets`.
- ~~**A stream nested inside an object stream is not rejected**~~ — **fixed**. 7.5.7: "an object
  stream shall not contain stream objects"; `Storage::resolve_ref` now returns `null` for such an
  entry (the `PrimitiveNotAllowed { found: STREAM }` the parser produces) instead of a `Stream`
  with a bogus `/Length`.
- ~~**`/DecodeParms` dropped / misplaced on write**~~ — **fixed**. `to_pdf_stream` collected a
  single dictionary in a loop over the filters and `assert!`ed that no second filter had
  parameters: a chain with two parameterised filters panicked on save, and with one it wrote
  that dictionary for filter 0 whatever the filter order was. `/DecodeParms` is now built per
  filter, `null` for the ones without parameters, as an array parallel to `/Filter` (7.4.2
  Table 5); a lone filter keeps the direct dictionary form. Test:
  `regressions::decode_parms_stay_aligned_with_the_filters`.
- ~~**`get_object_slice` can slice out of bounds**~~ — **fixed**. `/First` plus an entry offset
  can point past the decoded data or backwards; the range is validated before it is returned.
- ~~**`/FFilter` and `/FDecodeParms` length mismatch is silently tolerated**~~ — **fixed**.
  `StreamInfo::from_primitive` used to default missing external-file parameters to an empty
  dictionary, hiding a corrupt file. It now rejects a `/Filter` `/DecodeParms` or `/FFilter`
  `/FDecodeParms` length mismatch, accepts `null` holes in `/FDecodeParms` (they were parsed as
  dictionaries, so `[/Flate]` with `[null]` failed), and rejects `/FFilter` without an `/F`
  external file. Tests: `file_filters_and_their_params_must_line_up`,
  `file_filters_need_an_external_file`.

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
