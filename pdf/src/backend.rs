use crate::error::*;
use crate::parser::Lexer;
use crate::parser::read_xref_and_trailer_at;
use crate::xref::{XRef, XRefSection, XRefTable};
use crate::primitive::Dictionary;
use crate::object::*;
use std::ops::Deref;

use std::ops::{
    RangeFull,
    RangeFrom,
    RangeTo,
    Range,
};

pub const MAX_ID: u32 = 1_000_000;

pub trait Backend: Sized {
    fn read<T: IndexRange>(&self, range: T) -> Result<&[u8]>;
    //fn write<T: IndexRange>(&mut self, range: T) -> Result<&mut [u8]>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `range` could not be answered in full because those bytes are
    /// not here — a reader that fetches ranges on demand, holding only the
    /// part of the document some earlier pass asked for.
    ///
    /// `read` still returns what it does have (see `File`), so a parse of an
    /// object that is only partly here fails at the point the bytes stop.
    /// That failure says nothing about the document; the caller is going to
    /// fetch `range` and try again. [`crate::file::Storage::resolve_ref`]
    /// asks this question when a parse fails, and answers `null` for the
    /// object instead of an error, so the rest of a pass keeps going and
    /// finds the *other* ranges it needs in the same round.
    ///
    /// The default is `false`: a backend over a complete buffer never lies
    /// about what it has.
    fn is_incomplete(&self, range: Range<usize>) -> bool {
        let _ = range;
        false
    }

    /// Returns the offset of the beginning of the file, i.e., where the `%PDF-1.5` header is.
    /// (currently only used internally!)
    fn locate_start_offset(&self) -> Result<usize> {
        // Read from the beginning of the file, and look for the header.
        // Implementation note 13 in version 1.7 of the PDF reference says that Acrobat viewers
        // expect the header to be within the first 1KB of the file, so we do the same here.
        const HEADER: &[u8] = b"%PDF-";
        let buf = t!(self.read(..std::cmp::min(1024, self.len())));
        buf
            .windows(HEADER.len())
            .position(|window| window == HEADER)
            .ok_or_else(|| PdfError::Other{ msg: "file header is missing".to_string() })
    }

    /// Returns the value of startxref (currently only used internally!)
    fn locate_xref_offset(&self) -> Result<usize> {
        // locate the xref offset at the end of the file
        // `\nPOS\n%%EOF` where POS is the position encoded as base 10 integer.
        // u64::MAX has 20 digits + \n\n(2) + %%EOF(5) = 27 bytes max.
        //
        // Only a tail window is read: the same kilobyte `check_eof_marker`
        // inspects, grown fourfold while the marker is not in it (a file that
        // ends in padding, or one whose `startxref` sits under a big %%EOF
        // comment). A reader that fetches ranges by offset then asks for one
        // small chunk instead of the whole document.
        let len = self.len();
        let mut window = 1024.min(len);
        loop {
            let mut lexer = Lexer::new(t!(self.read(len - window .. len)));
            lexer.set_pos_from_end(0);
            if lexer.seek_substr_back(b"startxref").is_ok() {
                if let Ok(word) = lexer.next() {
                    if let Ok(pos) = word.to::<usize>() {
                        return Ok(pos);
                    }
                }
            }
            if window == len {
                bail!("the file does not end with a startxref offset");
            }
            window = window.saturating_mul(4).min(len);
        }
    }

    /// Used internally by File, but could also be useful for applications that want to look at the raw PDF objects.
    fn read_xref_table_and_trailer(&self, start_offset: usize, resolve: &impl Resolve) -> Result<(XRefTable, Dictionary)> {
        let xref_offset = t!(self.locate_xref_offset());
        // Every section offset read or being read, header-relative. A section
        // ends where the section that supersedes it begins, so this list is
        // what bounds the reads below: only the final section runs to the end
        // of the file.
        let mut seen = vec![xref_offset];
        let (xref_sections, trailer) = t!(self.read_xref_section_at(start_offset, xref_offset, None, resolve));

        let size = t!(trailer.get("Size")
            .ok_or_else(|| PdfError::MissingEntry {field: "Size".into(), typ: "XRefTable"})?
            .as_u32());

        if size > MAX_ID {
            bail!("too many objects");
        }
        let mut refs = XRefTable::new(size as ObjNr);
        for section in xref_sections {
            refs.add_entries_from(section)?;
        }

        // 7.5.8.4: in a hybrid-reference file a cross-reference stream named by
        // /XRefStm lists objects that the older section marks free with
        // generation 65535. Those sections are merged below, so remember what
        // the stream claimed and let nothing replace it.
        let mut from_xref_stream = vec![false; refs.len()];
        t!(self.add_xrefstm_entries(start_offset, &trailer, resolve, &mut refs, &mut from_xref_stream, &seen));

        let mut prev_trailer = {
            match trailer.get("Prev") {
                Some(p) => Some(t!(p.as_usize())),
                None => None
            }
        };
        trace!("READ XREF AND TABLE");
        while let Some(prev_xref_offset) = prev_trailer {
            if seen.contains(&prev_xref_offset) {
                bail!("xref offsets loop");
            }

            // `/Prev` names a section earlier in the file than the one that
            // named it, and the earliest section already seen is where this
            // one stops.
            let end = seen.iter().copied().filter(|&p| p > prev_xref_offset).min();
            let (xref_sections, trailer) = t!(self.read_xref_section_at(start_offset, prev_xref_offset, end, resolve));
            seen.push(prev_xref_offset);

            for section in xref_sections {
                refs.add_entries_from_skipping(section, &from_xref_stream)?;
            }
            t!(self.add_xrefstm_entries(start_offset, &trailer, resolve, &mut refs, &mut from_xref_stream, &seen));

            prev_trailer = {
                match trailer.get("Prev") {
                    Some(p) => {
                        let prev = t!(p.as_usize());
                        Some(prev)
                    }
                    None => None
                }
            };
        }
        refs.sanitize();

        Ok((refs, trailer))
    }

    /// Read the cross-reference section (table or stream) that starts `offset`
    /// bytes after the beginning of the file, i.e. after the `%PDF-` header.
    ///
    /// `end` bounds the read: a section never extends past the section that
    /// supersedes it, so when the update chain has already named a later one,
    /// the bytes up to it are all this section can occupy. `start_offset` +
    /// `end` is clamped to the file length, and a section that turns out to
    /// run past the hint is read again from `pos` to the end of the file, so a
    /// wrong hint in a damaged file costs a second read instead of data.
    /// `None` means "to the end of the file", which is what the section named
    /// by the final `startxref` gets, since nothing follows it.
    fn read_xref_section_at(&self, start_offset: usize, offset: usize, end: Option<usize>, resolve: &impl Resolve) -> Result<(Vec<XRefSection>, Dictionary)> {
        let pos = t!(start_offset.checked_add(offset).ok_or(PdfError::Invalid));
        if pos >= self.len() {
            bail!("XRef offset outside file bounds");
        }
        let bounded_end = match end {
            Some(end) if end > offset => Some(t!(start_offset.checked_add(end).ok_or(PdfError::Invalid)).min(self.len())),
            _ => None,
        };
        if let Some(stop) = bounded_end {
            let mut lexer = Lexer::with_offset(t!(self.read(pos .. stop)), pos);
            match read_xref_and_trailer_at(&mut lexer, resolve) {
                Ok(read) => return Ok(read),
                Err(e) if e.is_eof() => {
                    // A section that ends before the hint is wrong; a section
                    // that ends because its bytes are not here yet is not, and
                    // reading on would ask for the rest of the document.
                    if self.is_incomplete(pos .. stop) {
                        return Err(e);
                    }
                    debug!("xref section at {} runs past {}, reading to the end", pos, stop);
                }
                Err(e) => return Err(e),
            }
        }
        let mut lexer = Lexer::with_offset(t!(self.read(pos ..)), pos);
        read_xref_and_trailer_at(&mut lexer, resolve)
    }

    /// 7.5.8.4: the trailer of an update section in a hybrid-reference file may
    /// carry `/XRefStm`, the offset of a cross-reference stream. Its entries are
    /// consulted after the classic section and before the one `/Prev` names
    /// ("the cross-reference stream shall be consulted first"), so this is
    /// called between the two.
    ///
    /// The stream lists the objects that the file hides: the older section
    /// carries a free entry with generation 65535 for each of them, and the
    /// stream's in-use entry shall win. `from_xref_stream` records those object
    /// numbers so that the older sections cannot overwrite them.
    fn add_xrefstm_entries(
        &self,
        start_offset: usize,
        trailer: &Dictionary,
        resolve: &impl Resolve,
        refs: &mut XRefTable,
        from_xref_stream: &mut Vec<bool>,
        seen: &[usize],
    ) -> Result<()> {
        let offset = match trailer.get("XRefStm") {
            Some(p) => t!(p.as_usize()),
            None => return Ok(()),
        };
        // The stream belongs to the same update section as the table that
        // names it, so it stops where the next section seen so far begins.
        let end = seen.iter().copied().filter(|&p| p > offset).min();
        let (sections, stream_trailer) = match self.read_xref_section_at(start_offset, offset, end, resolve) {
            Ok(read) => read,
            Err(e) => {
                // The stream only duplicates information: every object it
                // mentions is also reachable through the older sections, minus
                // the hidden ones. Losing it is an xref error, not a reason to
                // refuse the file.
                if resolve.options().allow_xref_error {
                    warn!("ignoring unusable /XRefStm at {}: {}", offset, e);
                    return Ok(());
                }
                return Err(e);
            }
        };

        if let Some(size) = stream_trailer.get("Size").and_then(|p| p.as_u32().ok()) {
            if size > MAX_ID {
                bail!("too many objects");
            }
            refs.ensure_len(size as ObjNr);
        }

        for section in sections {
            let end = section.entries().map(|(nr, _)| nr as ObjNr + 1).max().unwrap_or(0);
            refs.ensure_len(end);

            let claimed: Vec<ObjNr> = section.entries()
                .filter(|(_, e)| matches!(e, XRef::Raw { .. } | XRef::Stream { .. }))
                .map(|(nr, _)| nr as ObjNr)
                .collect();
            refs.add_entries_from(section)?;

            if from_xref_stream.len() < refs.len() {
                from_xref_stream.resize(refs.len(), false);
            }
            for id in claimed {
                if let Some(flag) = from_xref_stream.get_mut(id as usize) {
                    *flag = true;
                }
            }
        }

        Ok(())
    }

    fn last_byte(&self) -> Option<u8> {
        let n = self.len();
        if n > 0 {
            self.read(n-1 .. n).ok().and_then(|s| s.get(0).cloned())
        } else {
            None
        }
    }
}


impl<T> Backend for T where T: Deref<Target=[u8]> { //+ DerefMut<Target=[u8]> {
    fn read<R: IndexRange>(&self, range: R) -> Result<&[u8]> {
        let r = t!(range.to_range(self.len()));
        Ok(&self[r])
    }
    /*
    fn write<R: IndexRange>(&mut self, range: R) -> Result<&mut [u8]> {
        let r = range.to_range(self.len())?;
        Ok(&mut self[r])
    }
    */
    fn len(&self) -> usize {
        (**self).len()
    }
}

pub trait BackendAppend: Backend + std::io::Write {}
impl<W: Backend + std::io::Write> BackendAppend for W {}

pub struct SplitData<B, W>(pub B, pub W);
impl<B: Backend, W: BackendAppend> Backend for SplitData<B, W> {
    fn read<T: IndexRange>(&self, range: T) -> Result<&[u8]> {
        let len_a = self.0.len();
        let len_b = self.1.len();
        let len = len_a + len_b;
        let Range { start, end } = t!(range.to_range(len));

        if start < len_a {
            self.0.read(start .. end.min(len_a))
        } else {
            self.1.read(start - len_a .. end - len_a)
        }
    }

    fn len(&self) -> usize {
        self.0.len() + self.1.len()
    }
}
impl<B, W: std::io::Write> std::io::Write for SplitData<B, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.1.write(buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.1.write_all(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.1.flush()
    }
}

/// `IndexRange` is implemented by Rust's built-in range types, produced
/// by range syntax like `..`, `a..`, `..b` or `c..d`.
pub trait IndexRange
{
    /// Start index (inclusive)
    fn start(&self) -> Option<usize>;

    /// End index (exclusive)
    fn end(&self) -> Option<usize>;

    /// `len`: the size of whatever container that is being indexed
    fn to_range(&self, len: usize) -> Result<Range<usize>> {
        match (self.start(), self.end()) {
            (None, None) => Ok(0 .. len),
            (Some(start), None) if start <= len => Ok(start .. len),
            (None, Some(end)) if end <= len => Ok(0 .. end),
            (Some(start), Some(end)) if start <= end && end <= len => Ok(start .. end),
            _ => Err(PdfError::ContentReadPastBoundary)
        }
    }
}


impl IndexRange for RangeFull {
    #[inline]
    fn start(&self) -> Option<usize> { None }
    #[inline]
    fn end(&self) -> Option<usize> { None }

}

impl IndexRange for RangeFrom<usize> {
    #[inline]
    fn start(&self) -> Option<usize> { Some(self.start) }
    #[inline]
    fn end(&self) -> Option<usize> { None }
}

impl IndexRange for RangeTo<usize> {
    #[inline]
    fn start(&self) -> Option<usize> { None }
    #[inline]
    fn end(&self) -> Option<usize> { Some(self.end) }
}

impl IndexRange for Range<usize> {
    #[inline]
    fn start(&self) -> Option<usize> { Some(self.start) }
    #[inline]
    fn end(&self) -> Option<usize> { Some(self.end) }
}

/// The bytes of a document: either read into memory, or mapped from the file.
///
/// [`FileOptions::open`](crate::file::FileOptions::open) returns this and, with
/// the `mmap` feature, maps the file: opening a large document then costs
/// address space but no resident memory until the parser touches a page. A
/// mapping is read-only; writing to it (an incremental update, see
/// [`Storage::save`](crate::file::Storage::save)) copies the bytes into memory
/// first, so a mapped file is never modified in place.
#[derive(Clone)]
pub enum Data {
    /// The whole file, read into memory.
    Owned(Vec<u8>),
    /// A read-only mapping of the file.
    #[cfg(feature = "mmap")]
    Mapped(std::sync::Arc<memmap2::Mmap>),
}

impl Data {
    /// `true` if the bytes come straight from a mapped file.
    pub fn is_mapped(&self) -> bool {
        match self {
            Data::Owned(_) => false,
            #[cfg(feature = "mmap")]
            Data::Mapped(_) => true,
        }
    }

    /// The bytes, as one slice.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Data::Owned(v) => v.as_slice(),
            #[cfg(feature = "mmap")]
            Data::Mapped(map) => map,
        }
    }

    /// Replace a mapping by a copy of it, so that the bytes can be written to.
    fn make_writable(&mut self) {
        if !matches!(self, Data::Owned(_)) {
            let copy = self.as_slice().to_vec();
            *self = Data::Owned(copy);
        }
    }

    /// Map `path` read-only. An empty file is not mapped (`mmap` refuses a
    /// zero length), the caller gets `Owned(vec![])` instead.
    ///
    /// Note that the mapping is only consistent as long as nothing else
    /// truncates or rewrites the file: `open` is for reading a document that
    /// is not being changed underneath the reader, which a caller that handles
    /// untrusted *paths* rather than untrusted *bytes* is in a position to
    /// guarantee.
    #[cfg(feature = "mmap")]
    pub fn map_file(path: &std::path::Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() == 0 {
            return Ok(Data::Owned(Vec::new()));
        }
        // SAFETY: memmap2 makes `map` unsafe because the mapping is undefined
        // behaviour if the file is truncated or written while it is mapped.
        // Nothing here writes to the file; see the note above on the caller's
        // side of that promise.
        let map = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Data::Mapped(std::sync::Arc::new(map)))
    }

    /// There is no mapping without the `mmap` feature.
    #[cfg(not(feature = "mmap"))]
    pub fn map_file(_path: &std::path::Path) -> Result<Self> {
        bail!("this build has no `mmap` feature")
    }
}

impl std::ops::Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::io::Write for Data {
    /// Writing appends: a mapping is copied into memory first.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.make_writable();
        match self {
            Data::Owned(v) => std::io::Write::write(v, buf),
            #[cfg(feature = "mmap")]
            Data::Mapped(_) => unreachable!("make_writable replaced the mapping"),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
