# content.rs — content streams and operators, against PDF 32000-1:2008 §7.8, §8, §9

Reviewed: 2026-11-01.

Status (commit "content: keep the Type 3 glyph width operator d0 (9.6.5)"):
the first two mismatches below are fixed; the rest are open. `d0` now has an
`Op::GlyphWidth { wx, wy }` variant, so the declared advance survives parsing and
serialising (`d1` is still dropped — next commit).

## Mismatches

- ~~**Unknown operator is a hard error**~~ — **fixed**. `ParseOptions::strict()` now sets
  `allow_invalid_ops = false` (it used to be `true` in both presets, so "strict" and
  "tolerant" behaved identically); `tolerant()` keeps `true`. So the same stream is
  reported by strict parsing and skipped by tolerant parsing, which is what 7.8.2's
  partial-recovery rule allows. Inside `BX … EX` unknown operators are ignored either way.
  (The other half of the old note still stands: the *lexer* bails on a stray `EI` / `ID`
  out of sentence, independent of the flag.)
- ~~**`d0` is parsed and dropped**~~ — **fixed**. 7.8.2 / Table A.1 define `d0` as a Type 3
  glyph-metric operator; it is now `Op::GlyphWidth { wx, wy }` (`c34`), so the declared glyph
  advance is no longer lost for Type 3 fonts whose `/Widths` disagree.
- **`d1` is parsed and dropped** (line 332, no `Op` variant). Same table: `d1` also carries the
  glyph bounding box. Open.
- **`Op::InlineImage` cannot be written** (`unimplemented!()` in the writer path, ~line 743):
  any content stream containing an inline image panics on `to_string`/save. 8.9.7 round-trip.
- **Inline image `/DP` is reused for every entry of `/F`** (`inline_image`, `decode_parms.clone()`),
  so `/F [/A85 /Fl] /DP [null << /Predictor 12 … >>]` applies the Flate params to the ASCII85
  stage too. 8.9.7 makes `/DP` parallel to `/F` like 7.4.2.
- **`Do0` accepted** (line 327) — not an operator in any edition; harmless but non-standard.
- Shading/clip state: `W`/`W*` emit a `Clip` op but the path is not marked as a clipping path
  before the next painting operator (8.5.4); an op consumer cannot tell "clip and fill" from
  "clip and discard".

## Checked, no mismatch

- Operator set: q Q cm w J j M d ri i gs m l c v y h re S s f F f* B B* b b* n W W* BT ET
  Tc Tw Tz TL Tf Tr Ts Td TD Tm T* Tj TJ `'` `"` Do sh CS cs SC SCN sc scn g G rg RG k K BI ID
  EI BMC BDC EMC MP DP BX EX — all present with the right operand counts.
- `v`/`y` control-point derivation from the current point; `TD` = `TL -ty` + `Td`; `'` = `T* Tj`;
  `"` = `aw Tw ac Tc string '`; text render modes 0-7 (8.2.3 Table 9), line join/cap 0-2.
- Inline image abbreviations `/BPC /CS /D /DP /F /H /IM /I /W` (8.9.7 Table 93) and the `ID`/`EI`
  framing, incl. the `/A85`-filtered case in the unit test.
- Content streams given as a single stream, an array of streams, or references to either
  (7.8.2) — all accepted, in order.
