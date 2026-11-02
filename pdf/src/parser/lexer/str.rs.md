# lexer/str.rs — against PDF 32000-1:2008

Reviewed: 2026-11-01.

Status: both mismatches below are **fixed** in this tree (commit "spec: fix wave 1");
cases `c03_*`, `fixtures_unknown_escape`, `c04_*` in `pdf/tests/spec_cases.rs`.

## Mismatches

- **Unknown escape emits NUL** (literal-string lexer, `next_lexeme`, `_ =>` arm). 7.3.4.2:
  "if the character following the REVERSE SOLIDUS is not one of those shown in Table 3,
  the REVERSE SOLIDUS shall be ignored". The crate pushes 0x00 and does not consume the
  character, so `(\d)` becomes `\0d` instead of `d`; writing the string back re-encodes
  the NUL.
- **Unescaped EOL not normalised to a single 0Ah**. 7.3.4.2: an end-of-line marker appearing
  within a literal string shall be treated as a single 0Ah byte. A CR and a CRLF are passed
  through raw (0Dh / 0Dh 0Ah), so extracted strings differ byte-for-byte from the spec's
  reading, and string equality/`/ID` comparisons can disagree with other readers.
- Hex strings: an odd number of digits should be padded with a trailing 0 (7.3.4.3); the
  padding behaviour here should be re-checked if hex-string extraction is relied on
  (the inline-image `/F [/A85 /Fl]` test path does not exercise it).

## Checked, no mismatch

- Balanced parentheses, `\(`, `\)`, `\\`, `\n \r \t \b \f`, `\ddd` octal (1-3 digits, mod 256),
  line continuation `\<EOL>`, nested parentheses, hex-string whitespace skipping.
