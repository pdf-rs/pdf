use std::io::{self, Write};
use std::ops::Deref;
use std::marker::PhantomData;
use byteorder::{ByteOrder, BigEndian, WriteBytesExt};
use itertools::Itertools;
use ordermap::OrderMap;
use types::*;
use object::{Object, RealizedRef, PromisedRef};
use xref::XRef;
use stream::Stream;

struct WriteCursor<W: Write> {
    inner:  W,
    pos:    u64
}
impl<W: Write> Write for WriteCursor<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write(buf) {
            Ok(len) => {
                self.pos += len as u64;
                Ok(len)
            },
            Err(e) => Err(e)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<W: Write> WriteCursor<W> {
    pub fn new(inner: W) -> WriteCursor<W> {
        WriteCursor {
            inner: inner,
            pos: 0
        }
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
}

pub struct PdfFile<W: Write> {
    cursor: WriteCursor<W>,
    refs:   Vec<XRef>
}
impl<W: Write> PdfFile<W> {
    pub fn new(out: W) -> io::Result<PdfFile<W>> {
        let mut cursor = WriteCursor::new(out);
        
        write!(&mut cursor, "%PDF-1.5\n")?;
    
        Ok(PdfFile {
            cursor: cursor,
            refs:   vec![XRef::Promised],
        })
    }

    pub fn add<T: Object>(&mut self, o: T) -> io::Result<RealizedRef<T>> {
        let id = self.refs.len() as u64;
        
        self.refs.push(XRef::Raw {
            offset:  self.cursor.position()
        });
        
        write!(self.cursor, "{} 0 obj\n", id)?;
        o.serialize(&mut self.cursor)?;
        write!(self.cursor, "\nendobj\n")?;
        
        Ok(RealizedRef {
            id:     id,
            obj:    Box::new(o),
        })
    }
    
    pub fn promise<T: Object>(&mut self) -> PromisedRef<T> {
        let id = self.refs.len() as u64;
        
        self.refs.push(XRef::Promised);
        
        PromisedRef {
            id:         id,
            _marker:    PhantomData
        }
    }
    pub fn fulfill<T: Object>(&mut self, promise: PromisedRef<T>, o: T)
     -> io::Result<RealizedRef<T>>
    {
        self.refs[promise.id as usize] = XRef::Raw {
            offset: self.cursor.position()
        };
        
        write!(self.cursor, "{} 0 obj\n", promise.id)?;
        o.serialize(&mut self.cursor)?;
        write!(self.cursor, "\nendobj\n")?;
        
        Ok(RealizedRef {
            id:     promise.id,
            obj:    Box::new(o),
        })
    }

    pub fn finish(mut self, catalog: RealizedRef<Catalog>) -> io::Result<()> {
        // remember offset of xref table
        let xref_offset: u64 = self.cursor.position();
        
        let max_stream_index: u32 = self.refs.iter().map(|r| match r {
            &XRef::Stream { index: i, .. } => i,
            _ => 0
        }).max().unwrap_or(0);
        let stream_index_bytes_empty = max_stream_index.leading_zeros() / 8;
        
        self.refs[0] = XRef::Raw {
            offset: xref_offset
        };
        
        let mut xref_stream = Stream::new("XRef");
        let offset_bytes_empty: u32 = xref_offset.leading_zeros() / 8;
        
        xref_stream.set("/Size", self.refs.len());
        xref_stream.set("/W", vec![1, 8 - offset_bytes_empty, 4 - stream_index_bytes_empty]);
        
        for ref_ in self.refs.iter() {
            let mut f2 = [0u8; 8];
            let mut f3 = [0u8; 4];
            
            let f1 = match ref_ {
                &XRef::Raw { offset: o } => {
                    BigEndian::write_u64(&mut f2, o);
                    1
                },
                &XRef::Stream { stream_id: id, index: i } => {
                    BigEndian::write_u64(&mut f2, id);
                    BigEndian::write_u32(&mut f3, i);
                    2
                },
                _ => panic!("a stream is missing")
            };
            xref_stream.write_u8(f1)?;
            xref_stream.write(&f2[offset_bytes_empty as usize ..])?;
            xref_stream.write(&f3[stream_index_bytes_empty as usize ..])?;
        }
        
        let xref_stream_pos = self.cursor.position();
        self.add(xref_stream)?;
        write!(&mut self.cursor, "trailer\n")?;
        write_dict!(&mut self.cursor,
            "/Size" << self.refs.len(),
            "/Root" << catalog
        );
        write!(&mut self.cursor, "\n{}\n%%EOF", xref_stream_pos)?;
        Ok(())
    }
}
