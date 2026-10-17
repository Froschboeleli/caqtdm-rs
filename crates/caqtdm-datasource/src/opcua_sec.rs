//! Symmetric OPC UA chunk for the signed-and-encrypted policies.
//!
//! Basic128Rsa15 and Basic256 sign with HMAC-SHA1. Basic256Sha256,
//! Aes128_Sha256_RsaOaep, and Aes256_Sha256_RsaPss sign with HMAC-SHA256.
//! The body is AES-CBC with PKCS7 and the IV prepended. SecurityPolicy#None
//! stays in `opcua_bin`.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::Sha256;

use crate::error::DsError;

pub const POLICY_NONE: &str = "http://opcfoundation.org/UA/SecurityPolicy#None";
pub const POLICY_BASIC128RSA15: &str = "http://opcfoundation.org/UA/SecurityPolicy#Basic128Rsa15";
pub const POLICY_BASIC256: &str = "http://opcfoundation.org/UA/SecurityPolicy#Basic256";
pub const POLICY_BASIC256SHA256: &str = "http://opcfoundation.org/UA/SecurityPolicy#Basic256Sha256";
pub const POLICY_AES128_SHA256_RSAOAEP: &str =
    "http://opcfoundation.org/UA/SecurityPolicy#Aes128_Sha256_RsaOaep";
pub const POLICY_AES256_SHA256_RSAPSS: &str =
    "http://opcfoundation.org/UA/SecurityPolicy#Aes256_Sha256_RsaPss";

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityPolicy {
    Basic128Rsa15,
    Basic256,
    Basic256Sha256,
    Aes128Sha256RsaOaep,
    Aes256Sha256RsaPss,
}

impl SecurityPolicy {
    pub fn parse(uri: &str) -> Option<Self> {
        match uri {
            POLICY_BASIC128RSA15 => Some(Self::Basic128Rsa15),
            POLICY_BASIC256 => Some(Self::Basic256),
            POLICY_BASIC256SHA256 => Some(Self::Basic256Sha256),
            POLICY_AES128_SHA256_RSAOAEP => Some(Self::Aes128Sha256RsaOaep),
            POLICY_AES256_SHA256_RSAPSS => Some(Self::Aes256Sha256RsaPss),
            _ => None,
        }
    }

    pub fn uri(self) -> &'static str {
        match self {
            Self::Basic128Rsa15 => POLICY_BASIC128RSA15,
            Self::Basic256 => POLICY_BASIC256,
            Self::Basic256Sha256 => POLICY_BASIC256SHA256,
            Self::Aes128Sha256RsaOaep => POLICY_AES128_SHA256_RSAOAEP,
            Self::Aes256Sha256RsaPss => POLICY_AES256_SHA256_RSAPSS,
        }
    }

    /// RSA-OAEP when true, RSA PKCS#1 v1.5 when false.
    pub fn oaep(self) -> bool {
        !matches!(self, Self::Basic128Rsa15)
    }

    /// RSA-OAEP-SHA256. The older OAEP policies hash with SHA-1.
    pub fn oaep_sha256(self) -> bool {
        matches!(self, Self::Aes256Sha256RsaPss)
    }

    /// RSASSA-PSS with SHA-256. The other policies use PKCS#1 v1.5.
    pub fn pss(self) -> bool {
        matches!(self, Self::Aes256Sha256RsaPss)
    }

    /// RSA-SHA256 when true, RSA-SHA1 when false. PSS still hashes with SHA-256.
    pub fn asym_sha256(self) -> bool {
        matches!(
            self,
            Self::Basic256Sha256 | Self::Aes128Sha256RsaOaep | Self::Aes256Sha256RsaPss
        )
    }

    pub fn sym_sha256(self) -> bool {
        self.asym_sha256()
    }

    pub fn sign_key_len(self) -> usize {
        match self {
            Self::Basic128Rsa15 => 16,
            Self::Basic256 => 24,
            Self::Basic256Sha256 | Self::Aes128Sha256RsaOaep | Self::Aes256Sha256RsaPss => 32,
        }
    }

    pub fn enc_key_len(self) -> usize {
        match self {
            Self::Basic128Rsa15 | Self::Aes128Sha256RsaOaep => 16,
            Self::Basic256 | Self::Basic256Sha256 | Self::Aes256Sha256RsaPss => 32,
        }
    }

    pub fn iv_len(self) -> usize {
        16
    }

    /// HMAC output length. SHA-1 policies keep all 20 bytes.
    pub fn mac_len(self) -> usize {
        if self.sym_sha256() {
            32
        } else {
            20
        }
    }

    pub fn nonce_len(self) -> usize {
        if matches!(self, Self::Basic128Rsa15) {
            16
        } else {
            32
        }
    }

    pub fn derived_len(self) -> usize {
        self.sign_key_len() + self.enc_key_len() + self.iv_len()
    }

    /// Extra bytes an RSA block spends on padding.
    pub fn rsa_overhead(self) -> usize {
        if self.oaep_sha256() {
            66
        } else if self.oaep() {
            42
        } else {
            11
        }
    }
}

pub struct SymKeys<'a> {
    pub enc: &'a [u8],
    pub sig: &'a [u8],
    pub iv: &'a [u8],
}

pub fn seal(
    policy: SecurityPolicy,
    token_id: u32,
    sequence: u32,
    request_id: u32,
    body: &[u8],
    keys: SymKeys<'_>,
) -> Result<Vec<u8>, DsError> {
    if keys.enc.len() != policy.enc_key_len() || keys.iv.len() != 16 {
        return Err(DsError::new("secure chunk key length"));
    }
    let cipher = aes_encrypt(keys.enc, keys.iv, body)?;
    let mut out = header(policy.uri(), token_id, sequence, request_id);
    out.extend_from_slice(keys.iv);
    out.extend_from_slice(&cipher);
    let tag = mac(policy, keys.sig, &out)?;
    out.extend_from_slice(&tag);
    Ok(out)
}

pub fn open(
    policy: SecurityPolicy,
    bytes: &[u8],
    key: &[u8],
    sig_key: &[u8],
) -> Result<(u32, u32, u32, Vec<u8>), DsError> {
    let mac_len = policy.mac_len();
    if bytes.len() < 32 + 16 + mac_len || key.len() != policy.enc_key_len() {
        return Err(DsError::new("secure chunk is short"));
    }
    let (signed, tag) = bytes.split_at(bytes.len() - mac_len);
    let expect = mac(policy, sig_key, signed)?;
    if expect.as_slice() != tag {
        return Err(DsError::new("secure chunk signature mismatch"));
    }
    let (uri, rest) = take_string(signed)?;
    if uri != policy.uri() {
        return Err(DsError::new(format!("unexpected security policy {uri}")));
    }
    if rest.len() < 12 + 16 {
        return Err(DsError::new("secure chunk header is short"));
    }
    let token_id = u32::from_le_bytes(rest[0..4].try_into().unwrap_or([0; 4]));
    let sequence = u32::from_le_bytes(rest[4..8].try_into().unwrap_or([0; 4]));
    let request_id = u32::from_le_bytes(rest[8..12].try_into().unwrap_or([0; 4]));
    let iv = &rest[12..28];
    let plain = aes_decrypt(key, iv, &rest[28..])?;
    Ok((token_id, sequence, request_id, plain))
}

fn aes_encrypt(key: &[u8], iv: &[u8], body: &[u8]) -> Result<Vec<u8>, DsError> {
    let mut buf = vec![0u8; body.len() + 16];
    buf[..body.len()].copy_from_slice(body);
    let cipher = if key.len() == 16 {
        let key: [u8; 16] = key.try_into().map_err(|_| DsError::new("aes key"))?;
        let iv: [u8; 16] = iv.try_into().map_err(|_| DsError::new("aes iv"))?;
        Aes128CbcEnc::new((&key).into(), (&iv).into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, body.len())
            .map_err(|_| DsError::new("secure chunk pad failed"))?
            .to_vec()
    } else {
        let key: [u8; 32] = key.try_into().map_err(|_| DsError::new("aes key"))?;
        let iv: [u8; 16] = iv.try_into().map_err(|_| DsError::new("aes iv"))?;
        Aes256CbcEnc::new((&key).into(), (&iv).into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, body.len())
            .map_err(|_| DsError::new("secure chunk pad failed"))?
            .to_vec()
    };
    Ok(cipher)
}

fn aes_decrypt(key: &[u8], iv: &[u8], cipher: &[u8]) -> Result<Vec<u8>, DsError> {
    let mut buf = cipher.to_vec();
    let plain = if key.len() == 16 {
        let key: [u8; 16] = key.try_into().map_err(|_| DsError::new("aes key"))?;
        let iv: [u8; 16] = iv.try_into().map_err(|_| DsError::new("aes iv"))?;
        Aes128CbcDec::new((&key).into(), (&iv).into())
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|_| DsError::new("secure chunk decrypt failed"))?
            .to_vec()
    } else {
        let key: [u8; 32] = key.try_into().map_err(|_| DsError::new("aes key"))?;
        let iv: [u8; 16] = iv.try_into().map_err(|_| DsError::new("aes iv"))?;
        Aes256CbcDec::new((&key).into(), (&iv).into())
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|_| DsError::new("secure chunk decrypt failed"))?
            .to_vec()
    };
    Ok(plain)
}

fn header(policy: &str, token_id: u32, sequence: u32, request_id: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(policy.len() as u32).to_le_bytes());
    out.extend_from_slice(policy.as_bytes());
    out.extend_from_slice(&token_id.to_le_bytes());
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&request_id.to_le_bytes());
    out
}

fn take_string(bytes: &[u8]) -> Result<(String, &[u8]), DsError> {
    if bytes.len() < 4 {
        return Err(DsError::new("policy length missing"));
    }
    let len = u32::from_le_bytes(bytes[0..4].try_into().unwrap_or([0; 4])) as usize;
    if bytes.len() < 4 + len {
        return Err(DsError::new("policy uri truncated"));
    }
    let text = std::str::from_utf8(&bytes[4..4 + len])
        .map_err(|_| DsError::new("policy uri is not utf-8"))?
        .to_string();
    Ok((text, &bytes[4 + len..]))
}

fn mac(policy: SecurityPolicy, key: &[u8], data: &[u8]) -> Result<Vec<u8>, DsError> {
    if policy.sym_sha256() {
        let mut mac = HmacSha256::new_from_slice(key).map_err(|_| DsError::new("bad hmac key"))?;
        mac.update(data);
        Ok(mac.finalize().into_bytes().to_vec())
    } else {
        let mut mac = HmacSha1::new_from_slice(key).map_err(|_| DsError::new("bad hmac key"))?;
        mac.update(data);
        Ok(mac.finalize().into_bytes().to_vec())
    }
}
