# pdf-rs [![Build Status](https://travis-ci.com/pdf-rs/pdf.svg?branch=master)](https://travis-ci.com/pdf-rs/pdf)
Read, alter and write PDF files.

**At the moment, you can only read PDF files.**

One easy way you can contribute is to add different PDF files to `tests/files` and see if they pass the tests (`cargo test`).

Feel free to contribute with ideas, issues or code! Please join [us on Zulip](https://type.zulipchat.com/#narrow/stream/209232-pdf) if you have any questions or problems.

# Building
See [fonts.md](fonts.md) if you want to support the 14 standard fonts.

# Workspace
This repository uses a Cargo Workspace and default members. This means by default only the `pdf` library is build.
To build additional parts, pass `--package=read` to build the subcrate you are interested in (here the `read` example).

# Examples
Currently we only have two very minimal examples `read` and `text`. However the library has grown a lot since they have been written.

# Inspect
There is a tool for visualizing a PDF file as an interactive hierarchy of primitives at [inspect-prim](https://github.com/pdf-rs/inspect-prim). Just clone and `cargo run`.

# Viewer
run it:
  `cargo run -p view --release --bin pdf_view YOUR_FILE.pdf`
Right now you can change pages with left and right arrow keys and zoom with '+' and '-'. Works for some files.

## [Try it in your browser](https://pdf-rs.github.io/view-wasm/)
