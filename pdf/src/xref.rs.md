# xref.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status (commit "xref: honour /XRefStm in hybrid-reference files"): the first two items below are
**fixed**.
- **section precedence** — `XRef::Stream` entries are no longer replaced by an older section
  (case `c16_newer_compressed_xref_entry_wins`), and the hybrid case is handled explicitly:
  `add_entries_from_skipping` leaves the entries of a cross-reference stream named by `/XRefStm`
  alone, whatever generation number the older section's free entry carries
  (`ensure_len` exists because the stream's `/Size` may cover more objects than the table was
  created with).
- **`/XRefStm`** — implemented in `backend.rs` (`add_xrefstm_entries`), case
  `c26_hybrid_xref_stream_entry_wins`; see `backend.rs.md`.
- the `get_gen_nr` panic is **not reachable from a parsed file**: xref sections only ever contain
  `Raw`/`Free`/`Stream` entries, so the `_ => panic!()` arm cannot be hit there. Left as is; the
  old wording ("reached from file.rs object loading") was wrong.

## Mismatches

- ~~**`/XRefStm` (hybrid-reference files) unsupported**~~ — **fixed** (7.5.8.4, see the status
  above): a trailer that carries `/XRefStm` had its stream ignored, so objects that exist only in
  that stream were unreachable.
- **Section precedence is wrong when merging** (`XRefTable::add_entries_from`, ~line 100-145).
  Entries are merged newest-section-first, but an existing `XRef::Stream` entry is *always*
  replaced by the older section's entry. 7.5.6/7.5.4 require the *earliest* (newest) section to
  win: an object stored compressed in the newest section can be re-pointed at an older definition,
  i.e. stale content is served.
- **`get_gen_nr` panics** (`_ => panic!()`) for entry kinds it does not model; reached from
  `file.rs` object loading. 7.5.4 has no "invalid" entry kind, but readers are expected to fall
  back to reconstruction (Annex C), not abort.

## Checked, no mismatch

- Written xref *stream* (`write_stream`): `/Type /XRef` via the derive, `/Size`, `/Index`
  defaults to `[0 size]`, `/W [1 a b]`, type 0/1/2 entries with big-endian fields of exactly
  the declared widths (7.5.8.2, 7.5.8.3). Unfiltered is legal (`/Filter` is optional).
- Free-list chain is rebuilt on write with `next_obj_nr`, `gen_nr 0xffff` for the head.
- Maximum field widths are computed from the actual offsets, so no truncation.
