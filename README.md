# pdf-rs [![Build Status](https://travis-ci.com/pdf-rs/pdf.svg?branch=master)](https://travis-ci.com/pdf-rs/pdf)
Read, alter and write PDF files.

**At the moment, you can only read PDF files.**

One easy way you can contribute is to add different PDF files to `tests/files` and see if they pass the tests (`cargo test`).

Feel free to contribute with ideas, issues or code! Please join [us on Zulip](https://type.zulipchat.com/#narrow/stream/209232-pdf) if you have any questions or problems.

# Building
The PDF standard references 14 standard fonts, which are not distributed with it.
Due to dubious practices by Adobe, it is not safe to publish them in the viewer.

You can download them [here](https://lbry.tv/pdf-standard-fonts.tar.bz:060d67b0d4f5ef9089853f3b314598e0e5d9c487) and unpack with
```
tar -xf fonts.tar.bz
```
placing the `fonts` directory in repository directory.

Alternativly you can run `./download_fonts.sh` to get them from [this old debian release of the Adobe PDF reader](http://ardownload.adobe.com/pub/adobe/reader/unix/9.x/9.5.5/enu/AdbeRdr9.5.5-1_i386linux_enu.deb). And `AdobePiStd.ttf` can be found on the internet as well:

Over all you will need in the `fonts` directory:
 - `CourierStd.otf`
 - `CourierStd-Bold.otf`
 - `CourierStd-Oblique.otf`
 - `CourierStd-BoldOblique.otf`
 - `MinionPro-Regular.otf`
 - `MinionPro-Bold.otf`
 - `MinionPro-It.otf`
 - `MinionPro-BoldIt.otf`
 - `MyriadPro-Regular.otf`
 - `MyriadPro-Bold.otf`
 - `MyriadPro-It.otf`
 - `MyriadPro-BoldIt.otf`
 - `SY______.PFB`
 - `AdobePiStd.otf`
 - `Arial-BoldMT.otf`
 - `ArialMT.ttf`
 - `Arial-ItalicMT.otf`


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