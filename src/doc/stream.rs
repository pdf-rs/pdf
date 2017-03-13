use doc::Object;


pub struct Stream {
    data:       Vec<u8>,
    dict:       HashMap<String, Primitive>
}
impl Stream {
    pub fn from_primitive(p: Primitive) -> Result<Self> {
        if let Primitive::Stream(stream) = p {
            let filter = stream.dict["Filter"];
            println!("filter: {:?}", filter);
            
            let raw = stream.content;
            // Uncompress/decode if there is a filter
            let data = match filter {
                Primitive::Name(ref s) => {
                    match s as &str {
                        "FlateDecode" => Reader::flat_decode(raw),
                        _ => bail!("NOT IMPLEMENTED: Filter type {}", s)
                    }
                },
                p => bail!("unsupported filter: {:?}", p)
            };
            
            Ok(Stream {
                data: data,
                dict: stream.dict
            })
        } else {
            bail!("not a Stream");
        }
    }

    pub fn new(type_: &'static str) -> Stream {
        Stream {
            data:   Vec::new(),
            type_:  type_,
            dict:   OrderMap::new()
        }
    }
    pub fn set<T: Object>(&mut self, key: &'static str, val: T) {
        let mut data = Vec::new();
        val.serialize(&mut data).unwrap();
        self.dict.insert(key, data);
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.data.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Object for Stream {
    fn serialize<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write(b"<<\n");
        write!(out, "  /Type {}\n", self.type_);
        for (k, v) in self.dict.iter() {
            out.write(b"  ")?;
            out.write(k.as_bytes())?;
            out.write(b" ")?;
            out.write(v)?;
            out.write(b"\n")?;
        }
        out.write(b">>\nstream\n")?;
        out.write(&self.data)?;
        out.write(b"\nendstream\n")?;
        Ok(())
    }
}



// TODO move out to decoding/encoding module
fn flat_decode(data: &[u8]) -> Vec<u8> {
    let mut inflater = InflateStream::from_zlib();
    let mut out = Vec::<u8>::new();
    let mut n = 0;
    while n < data.len() {
        let res = inflater.update(&data[n..]);
        if let Ok((num_bytes_read, result)) = res {
            n += num_bytes_read;
            out.extend(result);
        } else {
            res.unwrap();
        }
    }
    out
}
