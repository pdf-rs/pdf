/// PDF "cryptography" – This is why you don't write your own crypto.
use crate as pdf;
use crate::error::{PdfError, Result};
use crate::primitive::{Dictionary, PdfString};
use std::collections::HashMap;
use std::fmt;

const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

#[derive(Copy)]
pub struct Rc4 {
    i: u8,
    j: u8,
    state: [u8; 256],
}

impl Clone for Rc4 {
    fn clone(&self) -> Rc4 {
        *self
    }
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Rc4 {
        assert!(key.len() >= 1 && key.len() <= 256);
        let mut rc4 = Rc4 {
            i: 0,
            j: 0,
            state: [0; 256],
        };
        for (i, x) in rc4.state.iter_mut().enumerate() {
            *x = i as u8;
        }
        let mut j: u8 = 0;
        for i in 0..256 {
            j = j
                .wrapping_add(rc4.state[i])
                .wrapping_add(key[i % key.len()]);
            rc4.state.swap(i, j as usize);
        }
        rc4
    }
    fn next(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.state[self.i as usize]);
        self.state.swap(self.i as usize, self.j as usize);
        let k = self.state
            [(self.state[self.i as usize].wrapping_add(self.state[self.j as usize])) as usize];
        k
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
    #[pdf(key = "O")]
    o: PdfString,

    #[pdf(key = "U")]
    u: PdfString,

    #[pdf(key = "R")]
    r: u32,

    #[pdf(key = "P")]
    p: i32,

    #[pdf(key = "V")]
    v: i32,

    #[pdf(key = "Length", default = "40")]
    bits: u32,

    #[pdf(key = "CF")]
    crypt_filters: HashMap<String, CryptFilter>,

    #[pdf(key = "StmF")]
    default_crypt_filter: Option<String>,

    #[pdf(key = "EncryptMetadata", default = "true")]
    encrypt_metadata: bool,

    #[pdf(other)]
    _other: Dictionary,
}

#[derive(Object, Debug, Clone, Copy)]
pub enum CryptMethod {
    None,
    V2,
    AESV2,
}

#[derive(Object, Debug, Clone, Copy)]
pub enum AuthEvent {
    DocOpen,
    EFOpen,
}

#[derive(Object, Debug, Clone)]
#[pdf(Type = "CryptFilter?")]
pub struct CryptFilter {
    #[pdf(key = "CFM", default = "CryptMethod::None")]
    pub method: CryptMethod,

    #[pdf(key = "AuthEvent", default = "AuthEvent::DocOpen")]
    pub auth_event: AuthEvent,

    #[pdf(key = "Length")]
    pub length: Option<u32>,

    #[pdf(other)]
    _other: Dictionary,
}

pub struct Decoder {
    key_size: usize,
    key: [u8; 16], // maximum length
}
impl Decoder {
    pub fn default(dict: &CryptDict, id: &[u8]) -> Result<Decoder> {
        Decoder::from_password(dict, id, b"")
    }
    fn key(&self) -> &[u8] {
        &self.key[..self.key_size]
    }
    pub fn from_password(dict: &CryptDict, id: &[u8], pass: &[u8]) -> Result<Decoder> {
        let key_bits = match dict.v {
            1 | 2 | 3 => dict.bits,
            4 => {
                let default = dict
                    .crypt_filters
                    .get(dict.default_crypt_filter.as_ref().unwrap().as_str())
                    .unwrap();
                match default.method {
                    CryptMethod::V2 => default.length.map(|n| 8 * n).unwrap_or(dict.bits),
                    m => panic!("unimplemented crypt method {:?}", m),
                }
            }
            v => panic!("unsupported V value {}", v),
        };
        // 7.6.3.3 - Algorithm 2
        // get important data first
        let level = dict.r;
        let key_size = dbg!(key_bits as usize / 8);
        let o = dict.o.as_bytes();
        //let u = dict.u.as_bytes();
        let p = dict.p;

        // a) and b)
        let mut hash = md5::Context::new();
        if pass.len() < 32 {
            hash.consume(pass);
            hash.consume(&PADDING[..32 - pass.len()]);
        } else {
            hash.consume(&pass[..32]);
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
            for _ in 0..50 {
                data = *md5::compute(&data[..key_size]);
            }
        }

        let decoder = Decoder {
            key: data,
            key_size,
        };
        if decoder.check_password(dict, id) {
            Ok(decoder)
        } else {
            Err(PdfError::InvalidPassword)
        }
    }
    fn compute_u(&self, id: &[u8]) -> [u8; 16] {
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
        data
    }
    pub fn check_password(&self, dict: &CryptDict, id: &[u8]) -> bool {
        self.compute_u(id) == &dict.u.as_bytes()[..16]
    }
    pub fn decrypt(&self, id: u64, gen: u16, data: &mut [u8]) {
        // Algorithm 1
        // a) we have those already

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
    }
}
impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.key())
    }
}
