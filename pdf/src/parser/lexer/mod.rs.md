# lexer/mod.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01, crate `pdf` 0.10.0 @ 256f06f.

## Mismatches

- **FF is not whitespace** (`is_whitespace`, ~line 41). 7.2.2 Table 1 lists NUL, HT, LF, FF,
  CR, SP. Only HT/LF/CR/SP are skipped, so two tokens separated by a lone 0x0C are glued
  into one. Note `HexStringLexer::next_non_whitespace_char` in `lexer/str.rs` *does* skip
  0x0C — two different whitespace sets in the same crate.
- **Comments end at LF only** (comment skip, ~line 163). 7.2.4 defines EOL as CR, LF or
  CRLF, and a comment runs to the EOL; a comment ended by a bare CR swallows the next line.

## Checked, no mismatch

- Delimiters `()<>[]{}/%`, regular-character runs, numbers `.5`, `-.002`, `34.5`, `+17`;
  `true`/`false`/`null` keywords; name `/`, `//` (empty name); `<<` `>>` `[` `]` parsing.
- 7.2.3 token separation by delimiter / whitespace is otherwise respected.
