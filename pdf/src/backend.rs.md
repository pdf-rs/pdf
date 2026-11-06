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

## Read backend: `Data` and `OpenMode` (2026-11-07)

The backend is not necessarily a `Vec<u8>` any more. Stages 1–2 of 7.5.2 only
need bytes that can be indexed, so the file is presented as
[`Data`] (`Owned(Vec<u8>)` or, with the `mmap` feature, `Mapped(Arc<Mmap>)`),
which `Deref`s to `[u8]` and therefore gets the blanket `Backend` impl. There is
no spec clause behind this; it is the answer to the audit item "the whole file is
read into memory".

- `FileOptions::open` builds a `Data`. The default is `OpenMode::Read`: no
  document is mapped unless the caller asks for it. `OpenMode::Auto` maps only
  a file of at least `mmap_threshold` bytes (default `DEFAULT_MMAP_THRESHOLD`,
  64 KiB) — mapping a few kilobytes buys nothing — and `Mapped` asks for a
  mapping and errors without the feature. `open_owned` is the old signature, for
  callers that want `File<Vec<u8>>`; `mmap` itself stays an opt-in crate feature.
- A zero length file is never mapped (`mmap` refuses it) and a mapping that the
  file system refuses falls back to reading: both are `Data::Owned`.
- A mapping is read-only. `Data` implements `io::Write` by copying the bytes into
  memory on the first write, so an incremental update (`Storage::save`) and
  `File::save_to` work on a mapped document; `File::rebuild` never writes to the
  backend at all.
- What this does *not* change: the parser still needs a `&[u8]` of the whole
  document, so a file that can not be mapped still has to fit in memory, and a
  caller reading *untrusted* input should still cap the size
  (`FileOptions::max_file_size`) and preferably use a separate process with an
  address-space limit. `mmap` on an untrusted path adds one hazard of its own —
  truncating the file while it is mapped is undefined behaviour (documented on
  `Data::map_file`).

