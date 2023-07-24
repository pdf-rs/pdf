# pdf-rs [![test](https://github.com/pdf-rs/pdf/actions/workflows/test.yml/badge.svg)](https://github.com/pdf-rs/pdf/actions/workflows/test.yml) [![clippy&fmt](https://github.com/pdf-rs/pdf/actions/workflows/lint.yml/badge.svg)](https://github.com/pdf-rs/pdf/actions/workflows/lint.yml)
Read, alter and write PDF files.

Modifying and writing PDFs is still experimental.

One easy way you can contribute is to add different PDF files to `tests/files` and see if they pass the tests (`cargo test`).

Feel free to contribute with ideas, issues or code! Please join [us on Zulip](https://type.zulipchat.com/#narrow/stream/209232-pdf) if you have any questions or problems.

# Workspace
This repository uses a Cargo Workspace and default members. This means by default only the `pdf` library is build.
To build additional parts, pass `--package=read` to build the subcrate you are interested in (here the `read` example).

# Examples
Examples are located in `pdf/examples/` and can be executed using:

```
cargo run --example {content,metadata,names,read,text} -- <files/{choose a pdf}>
```

# Renderer and Viewer
A library for rendering PDFs via [Pathfinder](https://github.com/servo/pathfinder) and minimal viewer can be found [here](https://github.com/pdf-rs/pdf_render).

# Inspect
There is a tool for visualizing a PDF file as an interactive hierarchy of primitives at [inspect-prim](https://github.com/pdf-rs/inspect-prim). Just clone and `cargo run`.
