/// PDF "cryptography" – This is why you don't write your own crypto.

use crate as pdf;
use aes::cipher::generic_array::{sequence::Split, GenericArray};
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use aes::cipher::block_padding::{NoPadding, Pkcs7};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::fmt;
use std::collections::HashMap;
use datasize::DataSize;
use crate::object::PlainRef;
use crate::primitive::{Dictionary, PdfString, Name};
use crate::error::{PdfError, Result};

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

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
#[derive(Object, Debug, Clone, DataSize)]
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
    crypt_filters: HashMap<Name, CryptFilter>,

    #[pdf(key="StmF")]
    default_crypt_filter: Option<Name>,

    #[pdf(key="EncryptMetadata", default="true")]
    encrypt_metadata: bool,

    #[pdf(key = "OE")]
    oe: Option<PdfString>,

    #[pdf(key = "UE")]
    ue: Option<PdfString>,

    #[pdf(other)]
    _other: Dictionary
}

#[derive(Object, Debug, Clone, Copy, DataSize)]
pub enum CryptMethod {
    None,
    V2,
    AESV2,
    AESV3,
}

#[derive(Object, Debug, Clone, Copy, DataSize)]
pub enum AuthEvent {
    DocOpen,
    EFOpen
}

#[derive(Object, Debug, Clone, DataSize)]
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
    key: [u8; 32], // maximum length
    method: CryptMethod,
    /// A reference to the /Encrypt dictionary, if it is in an indirect
    /// object. The strings in this dictionary are not encrypted, so
    /// decryption must be skipped when accessing them.
    pub(crate) encrypt_indirect_object: Option<PlainRef>,
    /// A reference to the /Metadata dictionary, if it is an indirect
    /// object. If /EncryptMedata is set to false in the /Encrypt dictionary,
    /// then the strings in the /Metadata dictionary are not encrypted, so
    /// decryption must be skipped when accessing them.
    pub(crate) metadata_indirect_object: Option<PlainRef>,
    /// Whether the metadata is encrypted, as indicated by /EncryptMetadata
    /// in the /Encrypt dictionary.
    encrypt_metadata: bool,
}
impl Decoder {
    pub fn default(dict: &CryptDict, id: &[u8]) -> Result<Decoder> {
        Decoder::from_password(dict, id, b"")
    }

    fn key(&self) -> &[u8] {
        &self.key[.. std::cmp::min(self.key_size, 16)]
    }

    pub fn new(key: [u8; 32], key_size: usize, method: CryptMethod, encrypt_metadata: bool) -> Decoder {
        Decoder {
            key_size,
            key,
            method,
            encrypt_indirect_object: None,
            metadata_indirect_object: None,
            encrypt_metadata,
        }
    }

    pub fn from_password(dict: &CryptDict, id: &[u8], pass: &[u8]) -> Result<Decoder> {
        fn compute_u_rev_2(key: &[u8]) -> Vec<u8> {
            // algorithm 4
            let mut data = PADDING.to_vec();
            Rc4::encrypt(key, &mut data);
            data
        }

        fn check_password_rev_2(document_u: &[u8], key: &[u8]) -> bool {
            compute_u_rev_2(key) == document_u
        }

        fn compute_u_rev_3_4(id: &[u8], key: &[u8]) -> Vec<u8> {
            // algorithm 5
            // a) we derived the key already.

            // b)
            let mut hash = md5::Context::new();
            hash.consume(&PADDING);

            // c)
            hash.consume(id);

            // d)
            let mut data = *hash.compute();
            Rc4::encrypt(key, &mut data);

            // e)
            for i in 1u8..=19 {
                let mut key = key.to_owned();
                for b in &mut key {
                    *b ^= i;
                }
                Rc4::encrypt(&key, &mut data);
            }

            // f)
            data.to_vec()
        }

        fn check_password_rev_3_4(document_u: &[u8], id: &[u8], key: &[u8]) -> bool {
            compute_u_rev_3_4(id, key) == document_u[..16]
        }

        fn check_password_rc4(revision: u32, document_u: &[u8], id: &[u8], key: &[u8]) -> bool {
            if revision == 2 {
                check_password_rev_2(document_u, key)
            } else {
                check_password_rev_3_4(document_u, id, key)
            }
        }

        fn key_derivation_user_password_rc4(
            revision: u32,
            key_size: usize,
            dict: &CryptDict,
            id: &[u8],
            pass: &[u8],
        ) -> [u8; 32] {
            let o = dict.o.as_bytes();
            let p = dict.p;
            // 7.6.3.3 - Algorithm 2
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
            if revision >= 4 && !dict.encrypt_metadata {
                hash.consume([0xff, 0xff, 0xff, 0xff]);
            }

            // g)
            let mut data = *hash.compute();

            // h)
            if revision >= 3 {
                for _ in 0..50 {
                    data = *md5::compute(&data[..std::cmp::min(key_size, 16)]);
                }
            }

            let mut key = [0u8; 32];
            key[..16].copy_from_slice(&data);
            key
        }

        fn key_derivation_owner_password_rc4(
            revision: u32,
            key_size: usize,
            pass: &[u8],
        ) -> Result<Vec<u8>> {
            let mut hash = md5::Context::new();
            if pass.len() < 32 {
                hash.consume(pass);
                hash.consume(&PADDING[..32 - pass.len()]);
            } else {
                hash.consume(&pass[..32]);
            }

            if revision >= 3 {
                for _ in 0..50 {
                    let digest = *std::mem::replace(&mut hash, md5::Context::new()).compute();
                    hash.consume(digest);
                }
            }

            let digest = &hash.compute()[..key_size];
            Ok(digest.to_vec())
        }

        let (key_bits, method) = match dict.v {
            1 => (40, CryptMethod::V2),
            2 => (dict.bits, CryptMethod::V2),
            4 | 5 | 6 => {
                let default = dict
                    .crypt_filters
                    .get(try_opt!(dict.default_crypt_filter.as_ref()).as_str())
                    .ok_or_else(|| other!("missing crypt filter entry {:?}", dict.default_crypt_filter.as_ref()))?;

                match default.method {
                    CryptMethod::V2 | CryptMethod::AESV2 => (
                        default.length.map(|n| 8 * n).unwrap_or(dict.bits),
                        default.method,
                    ),
                    CryptMethod::AESV3 if dict.v == 5 => (
                        default.length.map(|n| 8 * n).unwrap_or(dict.bits),
                        default.method,
                    ),
                    m => err!(other!("unimplemented crypt method {:?}", m)),
                }
            }
            v => err!(other!("unsupported V value {}", v)),
        };
        let level = dict.r;
        if !(2..=6).contains(&level) {
            err!(other!("unsupported standard security handler revision {}", level))
        };
        if level <= 4 {
            let key_size = key_bits as usize / 8;
            let key = key_derivation_user_password_rc4(level, key_size, dict, id, pass);

            if check_password_rc4(level, dict.u.as_bytes(), id, &key[..std::cmp::min(key_size, 16)]) {
                let decoder = Decoder::new(key, key_size, method, dict.encrypt_metadata);
                Ok(decoder)
            } else {
                let password_wrap_key = key_derivation_owner_password_rc4(level, key_size, pass)?;
                let mut data = dict.o.as_bytes().to_vec();
                let rounds = if level == 2 { 1u8 } else { 20u8 };
                for round in 0..rounds {
                    let mut round_key = password_wrap_key.clone();
                    for byte in round_key.iter_mut() {
                        *byte ^= round;
                    }
                    Rc4::encrypt(&round_key, &mut data);
                }
                let unwrapped_user_password = data;

                let key = key_derivation_user_password_rc4(
                    level,
                    key_size,
                    dict,
                    id,
                    &unwrapped_user_password,
                );

                if check_password_rc4(level, dict.u.as_bytes(), id, &key[..key_size]) {
                    let decoder = Decoder::new(key, key_size, method, dict.encrypt_metadata);
                    Ok(decoder)
                } else {
                    Err(PdfError::InvalidPassword)
                }
            }
        } else if level == 5 || level == 6 {
            let u = dict.u.as_bytes();
            if u.len() != 48 {
                err!(format!(
                    "U in Encrypt dictionary should have a length of 48 bytes, not {}",
                    u.len(),
                )
                .into());
            }
            let user_hash = &u[0..32];
            let user_validation_salt = &u[32..40];
            let user_key_salt = &u[40..48];

            let o = dict.o.as_bytes();
            if o.len() != 48 {
                err!(format!(
                    "O in Encrypt dictionary should have a length of 48 bytes, not {}",
                    o.len(),
                )
                .into());
            }
            let owner_hash = &o[0..32];
            let owner_validation_salt = &o[32..40];
            let owner_key_salt = &o[40..48];

            let password_unicode =
                t!(String::from_utf8(pass.to_vec()).map_err(|_| PdfError::InvalidPassword));
            let password_prepped =
                t!(stringprep::saslprep(&password_unicode).map_err(|_| PdfError::InvalidPassword));
            let mut password_encoded = password_prepped.as_bytes();

            if password_encoded.len() > 127 {
                password_encoded = &password_encoded[..127];
            }

            let ue = t!(dict.ue.as_ref().ok_or_else(|| PdfError::MissingEntry {
                typ: "Encrypt",
                field: "UE".into(),
            }))
            .as_bytes()
            .to_vec();
            let oe = t!(dict.oe.as_ref().ok_or_else(|| PdfError::MissingEntry {
                typ: "Encrypt",
                field: "OE".into(),
            }))
            .as_bytes()
            .to_vec();

            let (intermediate_key, mut wrapped_key) = if level == 6 {
                let user_hash_computed =
                    Self::revision_6_kdf(password_encoded, user_validation_salt, b"");
                if user_hash_computed == user_hash {
                    (
                        Self::revision_6_kdf(password_encoded, user_key_salt, b"").into(),
                        ue,
                    )
                } else {
                    let owner_hash_computed =
                        Self::revision_6_kdf(password_encoded, owner_validation_salt, u);
                    if owner_hash_computed == owner_hash {
                        (
                            Self::revision_6_kdf(password_encoded, owner_key_salt, u).into(),
                            oe,
                        )
                    } else {
                        err!(PdfError::InvalidPassword);
                    }
                }
            } else {
                // level == 5

                let mut user_check_hash = Sha256::new();
                user_check_hash.update(password_encoded);
                user_check_hash.update(user_validation_salt);
                let user_hash_computed = user_check_hash.finalize();
                #[allow(clippy::branches_sharing_code)]
                if user_hash_computed.as_slice() == user_hash {
                    let mut intermediate_kdf_hash = Sha256::new();
                    intermediate_kdf_hash.update(password_encoded);
                    intermediate_kdf_hash.update(user_key_salt);
                    (intermediate_kdf_hash.finalize(), ue)
                } else {
                    let mut owner_check_hash = Sha256::new();
                    owner_check_hash.update(password_encoded);
                    owner_check_hash.update(owner_validation_salt);
                    owner_check_hash.update(u);
                    let owner_hash_computed = owner_check_hash.finalize();
                    if owner_hash_computed.as_slice() == owner_hash {
                        let mut intermediate_kdf_hash = Sha256::new();
                        intermediate_kdf_hash.update(password_encoded);
                        intermediate_kdf_hash.update(owner_key_salt);
                        intermediate_kdf_hash.update(u);
                        (intermediate_kdf_hash.finalize(), oe)
                    } else {
                        err!(PdfError::InvalidPassword);
                    }
                }
            };

            let zero_iv = GenericArray::from_slice(&[0u8; 16]);
            let key_slice = t!(Aes256CbcDec::new(&intermediate_key, zero_iv)
                .decrypt_padded_mut::<NoPadding>(&mut wrapped_key)
                .map_err(|_| PdfError::InvalidPassword));
            let mut key = [0u8; 32];
            key.copy_from_slice(key_slice);

            let decoder = Decoder::new(key,  32, method, dict.encrypt_metadata);
            Ok(decoder)
        } else {
            err!(format!("unsupported V value {}", level).into())
        }
    }

    fn revision_6_kdf(password: &[u8], salt: &[u8], u: &[u8]) -> [u8; 32] {
        let mut data = [0u8; (128 + 64 + 48) * 64];
        let mut data_total_len = 0;

        let mut sha256 = Sha256::new();
        let mut sha384 = Sha384::new();
        let mut sha512 = Sha512::new();

        let mut input_sha256 = Sha256::new();
        input_sha256.update(password);
        input_sha256.update(salt);
        input_sha256.update(u);
        let input = input_sha256.finalize();
        let (mut key, mut iv) = input.split();

        let mut block = [0u8; 64];
        let mut block_size = 32;
        (block[..block_size]).copy_from_slice(&input[..block_size]);

        let mut i = 0;
        while i < 64 || i < data[data_total_len - 1] as usize + 32 {
            let aes = Aes128CbcEnc::new(&key, &iv);
            let data_repeat_len = password.len() + block_size + u.len();
            data[..password.len()].copy_from_slice(password);
            data[password.len()..password.len() + block_size].copy_from_slice(&block[..block_size]);
            data[password.len() + block_size..data_repeat_len].copy_from_slice(u);
            for j in 1..64 {
                data.copy_within(..data_repeat_len, j * data_repeat_len);
            }
            data_total_len = data_repeat_len * 64;

            // The plaintext length will always be a multiple of the block size, unwrap is okay
            let encrypted = aes
                .encrypt_padded_mut::<NoPadding>(&mut data[..data_total_len], data_total_len)
                .unwrap();

            let sum: usize = encrypted[..16].iter().map(|byte| *byte as usize).sum();
            block_size = sum % 3 * 16 + 32;
            match block_size {
                32 => {
                    sha256.update(encrypted);
                    (block[..block_size]).copy_from_slice(&sha256.finalize_reset());
                }
                48 => {
                    sha384.update(encrypted);
                    (block[..block_size]).copy_from_slice(&sha384.finalize_reset());
                }
                64 => {
                    sha512.update(encrypted);
                    (block[..block_size]).copy_from_slice(&sha512.finalize_reset());
                }
                _ => unreachable!(),
            }

            key.copy_from_slice(&block[..16]);
            iv.copy_from_slice(&block[16..32]);

            i += 1;
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&block[..32]);
        hash
    }

    pub fn decrypt<'buf>(&self, id: PlainRef, data: &'buf mut [u8]) -> Result<&'buf [u8]> {
        if self.encrypt_indirect_object == Some(id) {
            // Strings inside the /Encrypt dictionary are not encrypted
            return Ok(data);
        }

        if !self.encrypt_metadata && self.metadata_indirect_object == Some(id) {
            // Strings inside the /Metadata dictionary are not encrypted when /EncryptMetadata is
            // false
            return Ok(data);
        }

        if data.is_empty() {
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
                key[n..n + 3].copy_from_slice(&id.id.to_le_bytes()[..3]);
                key[n + 3..n + 5].copy_from_slice(&id.gen.to_le_bytes()[..2]);

                // c)
                let key = *md5::compute(&key[..n + 5]);

                // d)
                Rc4::encrypt(&key[..(n + 5).min(16)], data);
                Ok(data)
            }
            CryptMethod::AESV2 => {
                // b)
                let mut key = [0; 32 + 5 + 4];
                let n = std::cmp::min(self.key_size, 16);
                key[..n].copy_from_slice(self.key());
                key[n..n + 3].copy_from_slice(&id.id.to_le_bytes()[..3]);
                key[n + 3..n + 5].copy_from_slice(&id.gen.to_le_bytes()[..2]);
                key[n + 5..n + 9].copy_from_slice(b"sAlT");

                // c)
                let key = *md5::compute(&key[..n + 9]);

                // d)
                let key = &key[..(n + 5).min(16)];
                if data.len() < 16 {
                    return Err(PdfError::DecryptionFailure);
                }
                let (iv, ciphertext) = data.split_at_mut(16);
                let cipher =
                    t!(Aes128CbcDec::new_from_slices(key, iv).map_err(|_| PdfError::DecryptionFailure));
                Ok(t!(cipher
                    .decrypt_padded_mut::<Pkcs7>(ciphertext)
                    .map_err(|_| PdfError::DecryptionFailure)))
            }
            CryptMethod::AESV3 => {
                if data.len() < 16 {
                    return Err(PdfError::DecryptionFailure);
                }
                let (iv, ciphertext) = data.split_at_mut(16);
                let cipher =
                    t!(Aes256CbcDec::new_from_slices(self.key(), iv).map_err(|_| PdfError::DecryptionFailure));
                Ok(t!(cipher
                    .decrypt_padded_mut::<Pkcs7>(ciphertext)
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

        let file = crate::file::FileOptions::uncached().load(data).unwrap();

        // PDF reference says strings in the encryption dictionary are "not
        // encrypted by the usual methods."
        assert_eq!(
            file.trailer.encrypt_dict.unwrap().o.as_ref(),
            b"owner pwd hash!!",
        );
    }
}
