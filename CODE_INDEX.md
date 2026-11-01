# pdf-rs/pdf — code index for agents

Upstream: https://github.com/pdf-rs/pdf · branch `master` · clone used for this index:
`/home/hematite/task-clones/pdf`, HEAD `256f06f` *"pdf: multiply the crypt filter /Length without overflowing"* (2026-10-26).
Crate `pdf` v0.10.0, MIT, edition 2021. ~13.7k lines of Rust under `pdf/src`.

Written 2026-11-01 by Hematite, for other agents working in this repo.

## Read this first: upstream contribution rules

`README.md` states the PR policy explicitly:

- **Code submitted upstream must be written by a human.** AI-written code (or the suspicion of it)
  gets the PR rejected. So: work here, produce patches/branches and review notes, but Sebastian
  (or another human) authors and submits anything that goes to GitHub.
- One PR containing multiple commits; no serial PRs.

`README.md` also names the sibling projects: `pdf_render` (pathfinder renderer) and
`inspect-prim` (interactive primitive hierarchy viewer). Rendering is **not** in this repo.

## Spec audit notes: `<source file>.md` next to the source

Every Rust file that was cross-referenced against **PDF 32000-1:2008** has a note file beside it,
named after the source file: e.g. `pdf/src/enc.rs` → `pdf/src/enc.rs.md`. Structure of each note:
*mismatches* (with clause numbers), *deliberate deviations*, *checked, no mismatch* (so an agent
knows what is already covered and does not re-audit it).

Notes written (2026-11-01):

| note | most consequential item |
|---|---|
| `pdf/src/parser/lexer/mod.rs.md` | FF (0x0C) missing from the whitespace set (7.2.2 Table 1) |
| `pdf/src/parser/lexer/str.rs.md` | unknown escape emits NUL; unescaped EOL not normalised to 0Ah (7.3.4.2) |
| `pdf/src/parser/mod.rs.md` | dict keys not `#XX`-decoded; null values kept (7.3.5, 7.3.9) |
| `pdf/src/parser/parse_xref.rs.md` | xref entry type > 2 errors instead of "null reference" (7.5.8.3) |
| `pdf/src/xref.rs.md` | `/XRefStm` unsupported; older section overrides newer entry (7.5.8.4, 7.5.6) |
| `pdf/src/file.rs.md` | panics on missing `startxref`; single crypt decoder for `/StmF`+`/StrF` |
| `pdf/src/enc.rs.md` | `Crypt` filter unimplemented; CCITT `K>=0` panics; PNG predictor ignores BitsPerComponent |
| `pdf/src/crypt.rs.md` | `/StrF` never read; per-stream `/Crypt /Name` ignored (7.6.5) |
| `pdf/src/content.rs.md` | unknown operator aborts the page; inline image cannot be written back |
| `pdf/src/object/stream.rs.md` | ObjStm `/First`+offsets unvalidated; header object numbers ignored (7.5.7) |
| `pdf/src/object/types/page.rs.md` | `inherit()` loops forever on a `/Parent` cycle |
| `pdf/src/object/types/pagesnode.rs.md` | `/Count` trusted, never verified (7.7.3.2) |
| `pdf/src/font.rs.md` | `/MissingWidth` ignored for simple fonts (9.6.2.1) |
| `pdf/src/primitive.rs.md` | written dates miss the closing apostrophe (7.9.4 Table 23) |

Not audited (no note file): `object/mod.rs` (traits), `object/types/*` other than page/pagesnode,
`encoding.rs`, `error.rs`, `object/function.rs` (PDF functions), `pdf_derive/`, `tests/`, `fuzz/`.
Full prose report with the same findings: `/home/hematite/report/pdf-rs-vs-pdf32000-2026-11-01.md`.

## Workspace layout

```
Cargo.toml            workspace: pdf, pdf_derive, examples (resolver = "2")
pdf/                  the library crate (everything below is relative to pdf/)
  pdf/src/            the library
  pdf/examples/       the 5 library examples: content, metadata, names, read, other_page_content
  pdf/tests/          integration.rs, write.rs, xref.rs, w_underflow.rs
  pdf/fuzz/           cargo-fuzz crate, targets: parse.rs, overflow.rs
pdf_derive/           proc-macro crate: #[derive(Object)] / #[derive(ObjectWrite)]
examples/             separate package `pdf-examples` with CLI bins: action, add_annot,
                      add_image, extract_page, fdf, form (examples/src/bin/)
files/                test PDF corpus used by integration tests (NOT under pdf/)
.github/workflows/test.yml   CI: stable toolchain, `cargo test --workspace`
```

`pdf_derive` in one line: `#[derive(Object)]` turns a struct into a typed view of a PDF
dictionary; `#[pdf(Type="XObject", Subtype="Image")]` adds header checks, `#[pdf(key="Width")]`
maps fields, `#[pdf(other)] Dictionary` catches leftover keys. Its `src/lib.rs` doc comment is the
manual. `#[derive(ObjectWrite)]` emits `to_primitive`. Types deriving these must also derive
`DataSize`, and most derive `Debug, Clone, DeepClone`.

## Module map (`pdf/src/`)

| file | lines | purpose |
|---|---|---|
| `lib.rs` | 29 | module list, re-exports `PdfError`; crate-wide allow()s |
| `error.rs` | 412 | `PdfError` (snafu), `Result`, `Context`, `t!`, `try_opt!`, data dump helper |
| `primitive.rs` | 1017 | untyped PDF values: `Primitive`, `Dictionary`, `Name`, `PdfString`, `PdfStream`, `Date` |
| `object/mod.rs` | 1081 | core traits (`Resolve`, `Object`, `ObjectWrite`, `Updater`, `FromDict`, `DeepClone`, `Trace`), refs (`PlainRef`, `Ref`, `RcRef`, `MaybeRef`, `Lazy`, `Shared`), `ParseOptions`, manual impls for std types |
| `object/stream.rs` | 420 | `Stream<I>`, `StreamInfo`, `ObjectStream` (objects packed in a stream) |
| `object/function.rs` | 719 | PDF functions: `Function` enum, sampled/interpolated (`SampledFunction`), PostScript (`PsFunc`), stitching |
| `object/color.rs` | 299 | `ColorSpace` (Device*, ICC, Lab, Indexed, Separation, Pattern) |
| `object/types.rs` | 585 | `Catalog`, `ViewerPreferences`, `Resources`, `Action`, `NameDictionary`, `FileSpec`, `EmbeddedFile`, `Rectangle`, `InfoDict`, … |
| `object/types/page.rs` | 270 | `PageRc`, `PagesRc`, `Page` (incl. inheritable `rotate()`), `PageLabel` |
| `object/types/pagesnode.rs` | 123 | `PagesNode` enum (Pages/Page), `PageTree` walking, counts |
| `object/types/form.rs` | 397 | AcroForm: fields, `Form`, `Annot` |
| `object/types/xobject.rs` | 214 | `XObject` enum, `ImageXObject` (width/height/bpc/color space/mask) |
| `object/types/{pattern,dest,outline,nametree,numbertree,structtree,graphicsstate}.rs` | 50–187 each | optional document features |
| `primitive.rs` + `object/mod.rs` | | the layer almost every bug passes through |
| `parser/mod.rs` | 432 | recursive-descent parser `parse`, `parse_with_lexer`, `parse_stream`, depth limit, `ParseFlags` |
| `parser/lexer/mod.rs` | 518 | `Lexer`, `Substr`, tokenising |
| `parser/lexer/str.rs` | 368 | PDF literal-string / hex-string decoders (`StringLexer`, `HexStringLexer`) |
| `parser/parse_object.rs` | 56 | `parse_indirect_object`, `parse_indirect_stream` ("N G obj … endobj") |
| `parser/parse_xref.rs` | 157 | xref table & xref stream + trailer parsing |
| `xref.rs` | 256 | `XRef` entry enum (`Raw`/`Stream`/`Free`/`Promised`/`Invalid`), `XRefTable`, `XRefSection`, `XRefInfo` |
| `file.rs` | 896 | `File`, `Storage`, `FileOptions`, caches, `Trailer`, `save_to`/`rebuild`, `scan()`, `resolver()` |
| `backend.rs` | 224 | `Backend` trait (byte storage indexing), `BackendAppend`, `SplitData`, `IndexRange`, `MAX_ID` |
| `enc.rs` | 729 | stream filters: `decode`/`encode`, `StreamFilter` enum, predictors, LZW/flate/ASCII85/hex/RunLength, JBIG2, CCITT fax, DCT dispatch |
| `crypt.rs` | 797 | `CryptDict`, `Decoder` (RC4 + AES-128/256), `Permissions`, `Rc4`, auth events |
| `encoding.rs` | 106 | font `Encoding` / `BaseEncoding` (WinAnsi, MacRoman, …) |
| `font.rs` | 833 | `Font`, `FontData`, `Widths`, `Type0Font`, `CIDFont`, `FontDescriptor`, `ToUnicodeMap`, CMap writing, UTF-16BE helpers |
| `content.rs` | 1214 | content streams: `Content`, `parse_ops`, `serialize_ops`, `Op` enum (all operators), `Matrix`, `Point`, `ViewRect`, `Color`, `FormXObject` |
| `build.rs` | 501 | writing: `PdfBuilder`, `PageBuilder`, `CatalogBuilder`, and `Importer`/`ImporterMap` for copying objects between documents |
| `any.rs` | 124 | `Any`/`AnySync` type-erased objects (`AnyObject`, `NoSize`, `WithSize`) used by the caches |
| `repair.rs` | 42 | xref-rebuilding sketch — **not declared in `lib.rs`, does not compile, dead** |
| `macros.rs` | 49 | `write_dict!` helpers — **also not in the module tree** |
| `data/t01_*.txt` | | tiny fixtures for filter decoding |

## Core abstractions (read these before touching anything)

`pdf/src/object/mod.rs`

- `Primitive` (`primitive.rs:18`): `Null | Integer | Real | Bool | Name | String | Array |
  Dictionary | Reference | Stream`. Every untyped value is one of these.
- `PlainRef { id, gen }` vs `Ref<T>` (typed) vs `RcRef<T>` (typed + resolved, `Arc`) vs
  `MaybeRef<T>` (inline or reference). `Lazy<T>` defers fetching. `Shared<T> = Arc<T>`.
- `Resolve`: `resolve_flags`, `resolve`, `get::<T>(Ref<T>) -> RcRef<T>`, `options()`,
  `stream_data`, `get_data_or_decode`. Implemented by `File` and `StorageResolver`; `NoResolve`
  errors on everything (used to parse without a document).
- `Updater`: `create`, `update`, `update_with`, `promise`/`fulfill` (`PromisedRef` for forward
  references while serialising). `NoUpdate` panics — never hand it to code that writes.
- `Object::from_primitive(p, resolve)` — read a typed value; `ObjectWrite::to_primitive(update)` —
  write one. `FromDict`/`ToDict` are the dictionary-shaped variants.
- `DeepClone`/`Cloner`: copying objects *into another document* while rewriting references.
  `Trace` visits the refs an object holds.
- `ParseOptions` (`object/mod.rs:37`): `tolerant()` vs `strict()` — error leniency, plus
  `object_size_limit` (10 MiB tolerant / 1 MiB strict), `max_img_dim`, `safe_jpeg`. Choose
  deliberately; `FileOptions::uncached()` defaults to **strict**.
- `ParseFlags` (bitflags in `parser/mod.rs`): what a parse position may yield (`ANY`, `DICT`,
  `ARRAY`, `INTEGER`, `REF`, `NAME`, `STRING`, `BOOL`, `NULL`, `NUMBER`, `STREAM`).

Parser: recursive descent with an explicit `max_depth` (16 by default via `Resolve::resolve`) and
`object_size_limit` on decompression — that is the main defence against hostile input. Keep both
when adding new recursion.

## Recipes

**Open and walk pages**

```rust
use pdf::file::FileOptions;
let file = FileOptions::cached().open("files/example.pdf")?;   // or .load(Vec<u8>) / .password(b"…")
let resolver = file.resolver();                                 // impl Resolve, borrows the file
for i in 0..file.num_pages() {
    let page = file.get_page(i)?;          // PageRc, derefs to Page
    if let Some(content) = &page.contents {
        let ops = content.operations(&resolver)?;   // Vec<Op>
    }
    let _ = page.rotate();                 // inheritable, walks /Parent
}
```
`FileOptions::cached()` needs the `cache` feature (on by default); `uncached()` skips the
`globalcache` object/stream caches. Passwords go through `FileOptions::password`; the test corpus
in `files/password_protected/` uses `userpassword` / `ownerpassword`.

**Modify / create**

`build.rs`: `PdfBuilder` + `PageBuilder`/`CatalogBuilder` for fresh documents, `Updater` impls on
`File` (`create`/`update`) for edits, `Importer`/`ImporterMap` to pull objects from another
document. Finish with `File::save_to(path)` or `File::rebuild() -> Vec<u8>` (incremental update:
appends a new xref section; `rebuild()` rewrites).

**Decode a stream by hand**

`enc::decode(data, &StreamFilter, &ParseOptions)` / `enc::encode`; `StreamFilter::from_kind_and_params`
maps `/Filter` names. Predictors via `PredictorType` + `unfilter`/`filter`. Encryption sits beside
it in `crypt.rs` (`Decoder`, `CryptDict`, per-object `/Crypt` filter).

**Content operators**

`content::parse_ops(data, &resolver)` → `Vec<Op>`; the `Op` enum (`content.rs:1047`) spells out
every operator (`MoveTo`, `LineTo`, `CurveTo`, `ShowText`, `SetMatrix`, …). Serialise with
`serialize_ops`. Geometry types (`Matrix`, `Point`, `ViewRect`, `Rect`) convert to/from
`euclid` (feature `euclid`, optional).

## Tests, fixtures, features

- `cargo test --workspace`. Integration tests: `pdf/tests/integration.rs` (open, page walk,
  passwords, stream decryption, `/Rotate` inheritance, `files/invalid` fuzz regressions,
  object streams), `tests/write.rs` (round-trip writing), `tests/xref.rs`,
  `tests/w_underflow.rs`; unit tests are inline
  `#[cfg(test)]` in most modules.
- Corpus: `files/` at the repo root. Adding a PDF there and making it pass is the documented
  easiest contribution. `files/invalid/` = must not panic (errors are fine).
- Features: `default = ["sync", "cache"]`; `mmap` (memmap2 load), `dump` (tempfile, error dumps),
  `cache` (globalcache; `CachedFile` alias). Several tests are `#[cfg(feature = "cache")]`.
- `cargo test` alone builds only the `pdf` lib; examples are separate `[[example]]` targets.

## Examples (`pdf/examples/` + `examples/src/bin/`)

- Library examples (declared as `[[example]]` in `pdf/Cargo.toml`): `read` (dump structure),
  `content` (page content ops), `metadata` (Info dict), `names` (name tree),
  `other_page_content`. Run from `pdf/`: `cargo run --example read -- ../files/example.pdf`.
- `pdf-examples` package bins: `extract_page`, `add_annot`, `add_image`, `action`, `form`, `fdf`.
  Run with `cargo run -p pdf-examples --bin extract_page -- …` from the workspace root.

## Fuzzing

`pdf/fuzz` (cargo-fuzz, not part of the workspace): `parse` target parses arbitrary bytes,
`overflow` targets arithmetic in length/offset handling. `files/invalid/` holds the
crashes found this way; the `invalid_pdfs` test only asserts "no panic".

## Gotchas that bite

1. **`repair.rs` is dead.** Xref repair is *not* wired in; a broken xref fails rather than being
   rebuilt. If a task says "repair", it means writing that (see `file.rs` `load_storage_and_trailer_password`).
2. **`NoResolve`/`NoUpdate` panic or error deliberately** — a stale import often means a test
   constructed a document with no backend.
3. **Caches must be invalidated** after an update: `file.rs` clears `storage.cache` in `save`;
   forgetting it serves stale objects.
4. **`PromisedRef`**: objects written before their target exists; `fulfill` must be called or the
   file is inconsistent.
5. **Strict by default.** Most real-world PDFs need `ParseOptions::tolerant()`; the examples and
   tests show the difference.
6. **Encrypted files hide errors**: dict structure parses fine with a wrong key, only strings and
   streams decrypt wrongly — always exercise a stream (see `decrypt_streams` test).
7. **`DataSize` is a hard bound** on nearly every generic (`Resolve::get`, `Cache`); new wrapper
   types need `#[derive(DataSize)]` or they won't compile in those positions.
8. Recent upstream fixes to respect: `256f06f` crypt `/Length` overflow, `#293` unsigned `/P`,
   `#297` `/Contents` as an indirect ref to an array. Don't regress those.

## Where to change what

| task | file(s) |
|---|---|
| new/changed page attribute | `object/types/page.rs` (+ `pagesnode.rs` if inheritable) |
| new dictionary type | define struct + `#[derive(Object, ObjectWrite, DataSize, Debug, Clone, DeepClone)]` in `object/types*.rs`, add to `types.rs` |
| parser bug / new token form | `parser/mod.rs`, `parser/lexer/mod.rs` |
| xref / trailer handling | `parser/parse_xref.rs`, `xref.rs`, `file.rs` |
| new stream filter | `enc.rs` (`StreamFilter`, `decode`, `encode`) |
| encryption | `crypt.rs` |
| text extraction behaviour | `content.rs` (`Op::ShowText` etc.), `font.rs` (`ToUnicodeMap`, widths) |
| writing/serialising | `object/mod.rs` (`ObjectWrite`), `primitive.rs`, `file.rs` (`save`, `rebuild`) |
| document building API | `build.rs` |
| error variants | `error.rs` (snafu; keep `Result` alias) |
