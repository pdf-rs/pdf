/// PDF "cryptography" – This is why you don't write your own crypto.

use crate as pdf;
use aes::Aes128;
use block_modes::{BlockMode, Cbc};
use block_modes::block_padding::Pkcs7;
use std::fmt;
use std::collections::HashMap;
use crate::object::PlainRef;
use crate::primitive::{Dictionary, PdfString};
use crate::error::{PdfError, Result};

const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41,
    0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
    0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A
];

#[derive(Copy)]
pub struct Rc4 {
    i: u8,
    j: u8,
    state: [u8; 256]
}

impl Clone for Rc4 { fn clone(&self) -> Rc4 { *self } }

impl Rc4 {
    pub fn new(key: &[u8]) -> Rc4 {
        assert!(!key.is_empty() && key.len() <= 256);
        let mut rc4 = Rc4 { i: 0, j: 0, state: [0; 256] };
        for (i, x) in rc4.state.iter_mut().enumerate() {
            *x = i as u8;
        }
        let mut j: u8 = 0;
        for i in 0..256 {
            j = j.wrapping_add(rc4.state[i]).wrapping_add(key[i % key.len()]);
            rc4.state.swap(i, j as usize);
        }
        rc4
    }
    fn next(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.state[self.i as usize]);
        self.state.swap(self.i as usize, self.j as usize);
        self.state[(self.state[self.i as usize].wrapping_add(self.state[self.j as usize])) as usize]
    }
    pub fn encrypt(key: &[u8], data: &mut [u8]) {
        let mut rc4 = Rc4::new(key);
        for b in data.iter_mut() {
            *b ^= rc4.next();
        }
    }
}

/// 7.6.1 Table 20 + 7.6.3.2 Table 21
#[derive(Object, Debug, Clone)]
pub struct CryptDict {
    #[pdf(key="O")]
    o: PdfString,
    
    #[pdf(key="U")]
    u: PdfString,
    
    #[pdf(key="R")]
    r: u32,
    
    #[pdf(key="P")]
    p: i32,
    
    #[pdf(key="V")]
    v: i32,

    #[pdf(key="Length", default="40")]
    bits: u32,

    #[pdf(key="CF")]
    crypt_filters: HashMap<String, CryptFilter>,

    #[pdf(key="StmF")]
    default_crypt_filter: Option<String>,

    #[pdf(key="EncryptMetadata", default="true")]
    encrypt_metadata: bool,

    #[pdf(other)]
    _other: Dictionary
}

#[derive(Object, Debug, Clone, Copy)]
pub enum CryptMethod {
    None,
    V2,
    AESV2
}

pub enum StandardSecurityHandlerRevision {
    R2,
    R3,
    R4,
}

#[derive(Object, Debug, Clone, Copy)]
pub enum AuthEvent {
    DocOpen,
    EFOpen
}

#[derive(Object, Debug, Clone)]
#[pdf(Type="CryptFilter?")]
pub struct CryptFilter {
    #[pdf(key="CFM", default="CryptMethod::None")]
    pub method: CryptMethod,

    #[pdf(key="AuthEvent", default="AuthEvent::DocOpen")]
    pub auth_event: AuthEvent,

    #[pdf(key="Length")]
    pub length: Option<u32>,

    #[pdf(other)]
    _other: Dictionary
}


pub struct Decoder {
    key_size: usize,
    key: [u8; 16], // maximum length
    method: CryptMethod,
    revision: StandardSecurityHandlerRevision,
    /// A reference to the /Encrypt dictionary, if it is in an indirect
    /// object. The strings in this dictionary are not encrypted, so
    /// decryption must be skipped when accessing them.
    pub(crate) encrypt_indirect_object: Option<PlainRef>,
}
impl Decoder {
    pub fn default(dict: &CryptDict, id: &[u8]) -> Result<Decoder> {
        Decoder::from_password(dict, id, b"")
    }
    fn key(&self) -> &[u8] {
        &self.key[.. self.key_size]
    }
    pub fn from_password(dict: &CryptDict, id: &[u8], pass: &[u8]) -> Result<Decoder> {
        let (key_bits, method) = match dict.v {
            1 => (40, CryptMethod::V2),
            2 => (dict.bits, CryptMethod::V2),
            4 => {
                let default = dict.crypt_filters.get(dict.default_crypt_filter.as_ref().unwrap().as_str()).unwrap();
                match default.method {
                    CryptMethod::V2 => {
                        (
                            default.length.map(|n| 8 * n).unwrap_or(dict.bits),
                            CryptMethod::V2,
                        )
                    },
                    CryptMethod::AESV2 => {
                        (
                            default.length.map(|n| 8 * n).unwrap_or(dict.bits),
                            CryptMethod::AESV2,
                        )
                    }
                    m => err!(format!("unimplemented crypt method {:?}", m).into()),
                }
            },
            v => err!(format!("unsupported V value {}", v).into()),
        };
        let revision = match dict.r {
            2 => StandardSecurityHandlerRevision::R2,
            3 => StandardSecurityHandlerRevision::R3,
            4 => StandardSecurityHandlerRevision::R4,
            other => err!(format!("unsupported standard security handler revision {}", other).into()),
        };
        // 7.6.3.3 - Algorithm 2
        // get important data first
        let level = dict.r;
        let key_size = key_bits as usize / 8;
        let o = dict.o.as_bytes();
        //let u = dict.u.as_bytes();
        let p = dict.p;

        // a) and b)
        let mut hash = md5::Context::new();
        if pass.len() < 32 {
            hash.consume(pass);
            hash.consume(&PADDING[.. 32 - pass.len()]);
        } else {
            hash.consume(&pass[.. 32]);
        }

        // c)
        hash.consume(o);

        // d)
        hash.consume(p.to_le_bytes());

        // e)
        hash.consume(id);

        // f)
        if level >= 4 && !dict.encrypt_metadata {
            hash.consume([0xff, 0xff, 0xff, 0xff]);
        }

        if !dict.encrypt_metadata {
            warn!("metadata not encrypted. this is not implemented yet!");
        }

        // g)
        let mut data = *hash.compute();

        // h)
        if level >= 3 {
            for _ in 0 .. 50 {
                data = *md5::compute(&data[.. key_size]);
            }
        }

        let decoder = Decoder {
            key: data,
            key_size,
            method,
            revision,
            encrypt_indirect_object: None,
        };
        if decoder.check_password(dict, id) {
            Ok(decoder)
        } else {
            Err(PdfError::InvalidPassword)
        }
    }
    fn compute_u(&self, id: &[u8]) -> Vec<u8> {
        match self.revision {
            StandardSecurityHandlerRevision::R2 => {
                // algorithm 4
                let mut data = PADDING.to_vec();
                Rc4::encrypt(self.key(), &mut data);
                data
            }
            StandardSecurityHandlerRevision::R3 | StandardSecurityHandlerRevision::R4 => {
                // algorithm 5
                // a) we created self already.

                // b)
                let mut hash = md5::Context::new();
                hash.consume(&PADDING);

                // c)
                hash.consume(id);

                // d)
                let mut data = *hash.compute();
                Rc4::encrypt(self.key(), &mut data);

                // e)
                for i in 1u8..=19 {
                    let mut key = self.key;
                    for b in &mut key {
                        *b ^= i;
                    }
                    Rc4::encrypt(&key[..self.key_size], &mut data);
                }

                // f)
                data.to_vec()
            }
        }
    }
    pub fn check_password(&self, dict: &CryptDict, id: &[u8]) -> bool {
        let computed_u = self.compute_u(id);
        let document_u = dict.u.as_bytes();
        match self.revision {
            StandardSecurityHandlerRevision::R2 => computed_u == document_u,
            StandardSecurityHandlerRevision::R3 | StandardSecurityHandlerRevision::R4 => {
                computed_u == &document_u[..16]
            }
        }
    }
    pub fn decrypt<'buf>(&self, id: u64, gen: u16, data: &'buf mut [u8]) -> Result<&'buf [u8]> {
        if self.encrypt_indirect_object == Some(PlainRef { id, gen }) {
            // Strings inside the /Encrypt dictionary are not encrypted
            return Ok(data);
        }

        // Algorithm 1
        // a) we have those already

        match self.method {
            CryptMethod::None => unreachable!(),
            CryptMethod::V2 => {
                // b)
                let mut key = [0; 16 + 5];
                let n = self.key_size;
                key[..n].copy_from_slice(self.key());
                key[n..n + 3].copy_from_slice(&id.to_le_bytes()[..3]);
                key[n + 3..n + 5].copy_from_slice(&gen.to_le_bytes()[..2]);

                // c)
                let key = *md5::compute(&key[..n + 5]);

                // d)
                Rc4::encrypt(&key[..(n + 5).min(16)], data);
                Ok(data)
            }
            CryptMethod::AESV2 => {
                // b)
                let mut key = [0; 16 + 5 + 4];
                let n = self.key_size;
                key[..n].copy_from_slice(self.key());
                key[n..n + 3].copy_from_slice(&id.to_le_bytes()[..3]);
                key[n + 3..n + 5].copy_from_slice(&gen.to_le_bytes()[..2]);
                key[n + 5..n + 9].copy_from_slice(b"sAlT");

                // c)
                let key = *md5::compute(&key[..n + 9]);

                // d)
                type Aes128Cbc = Cbc<Aes128, Pkcs7>;
                let key = &key[..(n + 5).min(16)];
                let (iv, ciphertext) = data.split_at_mut(16);
                let cipher =
                    t!(Aes128Cbc::new_var(key, iv).map_err(|_| PdfError::DecryptionFailure));
                Ok(t!(cipher
                    .decrypt(ciphertext)
                    .map_err(|_| PdfError::DecryptionFailure)))
            }
        }
    }
}
impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("key", &self.key())
            .field("method", &self.method)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unencrypted_strings() {
        let data_prefix = b"%PDF-1.5\n\
            1 0 obj\n\
            << /Type /Catalog /Pages 2 0 R >>\n\
            endobj\n\
            2 0 obj\n\
            << /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
            endobj\n\
            3 0 obj\n\
            << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>\n\
            endobj\n\
            4 0 obj\n\
            << /Length 0 >>\n\
            stream\n\
            endstream\n\
            endobj\n\
            5 0 obj\n\
            <<\n\
                /V 4\n\
                /CF <<\n\
                    /StdCF << /Type /CryptFilter /CFM /V2 >>\n\
                >>\n\
                /StmF /StdCF\n\
                /StrF /StdCF\n\
                /R 4\n\
                /O (owner pwd hash!!)\n\
                /U <E721D9D63EC4E7BD4DA6C9F0E30C8290>\n\
                /P -4\n\
            >>\n\
            endobj\n\
            xref\n\
            1 5\n";
        let mut data = data_prefix.to_vec();
        for obj_nr in 1..=5 {
            let needle = format!("\n{} 0 obj\n", obj_nr).into_bytes();
            let offset = data_prefix
                .windows(needle.len())
                .position(|w| w == needle)
                .unwrap()
                + 1;
            let mut line = format!("{:010} {:05} n\r\n", offset, 0).into_bytes();
            assert_eq!(line.len(), 20);
            data.append(&mut line);
        }
        let trailer_snippet = b"trailer\n\
            <<\n\
                /Size 6\n\
                /Root 1 0 R\n\
                /Encrypt 5 0 R\n\
                /ID [<DEADBEEF> <DEADBEEF>]\n\
            >>\n\
            startxref\n";
        data.extend_from_slice(trailer_snippet);
        let xref_offset = data_prefix
            .windows("xref".len())
            .rposition(|w| w == b"xref")
            .unwrap();
        data.append(&mut format!("{}\n%%EOF", xref_offset).into_bytes());

        let file = crate::file::File::from_data(data).unwrap();

        // PDF reference says strings in the encryption dictionary are "not
        // encrypted by the usual methods."
        assert_eq!(
            file.trailer.encrypt_dict.unwrap().o.as_ref(),
            b"owner pwd hash!!",
        );
    }
}
