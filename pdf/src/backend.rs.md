# backend.rs — against PDF 32000-1:2008 §7.5.8

Reviewed: 2026-11-02 (was missing from the audit; the xref assembly lives here, not in
`xref.rs`).

## Status (commit "xref: honour /XRefStm in hybrid-reference files")

- **`/XRefStm` is implemented** (`add_xrefstm_entries`, case
  `c26_hybrid_xref_stream_entry_wins`, fixture `c26-hybrid-xrefstm.pdf`; PyMuPDF renders it).
  The lookup order inside an update section is now the one 7.5.8.4 asks for: the classic section,
  then the cross-reference stream that its trailer's `/XRefStm` names, then the section `/Prev`
  names. The object numbers the stream claims as in use are remembered in
  `from_xref_stream` and `XRefTable::add_entries_from_skipping` refuses to overwrite them, which is
  what defeats the free entry with generation 65535 that hides an object in the older section.
- **The two "hidden object" failure modes** are covered by that single rule: without it the older
  free entry won the generation comparison (65535 > 0) and the object resolved as
  `FreeObject`/`NullRef`. Not modelled are the extra entries a hybrid file may put in the stream
  only for the *benefit* of readers that ignore `/XRefStm` (7.5.8.4 says the stream "shall
  contain entries for all objects in the file"); a file that gets the duplication wrong still
  reads, because the classic sections are merged first.
- An unusable `/XRefStm` (bad offset, broken stream) is reported through `warn!` and ignored when
  `ParseOptions::allow_xref_error` is set, and is a hard error otherwise. 7.5.8.4 makes the stream
  an optimisation, and losing it costs only the hidden objects.
- `/XRefStm` in the *first* (main) trailer is honoured too. The spec says it shall not be there,
  but tolerating it costs one call and cannot change the result for a conforming file.

## Checked, no mismatch

- `locate_xref_offset` reads the last `startxref`, as 7.5.5/C.4 expect, and reports a missing one
  as an error rather than panicking.
- Section offsets are all taken relative to `start_offset` (the `%PDF-` header position), so a
  file with leading junk is still read consistently — the offsets in the file count from the
  header for a conforming file, and `start_offset` added to them is the tolerant reading.
- `/Prev` chains are bounded by a `seen` list, so a loop is an error (7.5.5's "shall not" is
  treated as a corruption, not a crash).
- `/Size` is capped at `MAX_ID` (1e6) before the table is allocated (7.5.8.2's `/Size` is
  attacker-controlled).

## Not this file's business, but reached from it

- The whole file is read into memory before `read_xref_table_and_trailer` (`file.rs`,
  `FileOptions::open`); `/Size` and the subsection counts are capped here, the file length is not.
