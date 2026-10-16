//! OPC UA OpenSecureChannel for the certificate policies.
//!
//! The OPN chunk carries the sender certificate and the receiver certificate
//! thumbprint (SHA-1). Basic128Rsa15 signs with RSA-SHA1 and encrypts with
//! RSA PKCS#1 v1.5. Basic256 signs with RSA-SHA1 and encrypts with RSA-OAEP.
//! Basic256Sha256 and Aes128_Sha256_RsaOaep sign with RSA-SHA256 and encrypt
//! with RSA-OAEP. Both sides then run P_SHA1 or P_SHA256 over the nonces.

use rand::rngs::OsRng;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::EncodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::traits::PublicKeyParts;
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use x509_parser::prelude::*;
use x509_parser::public_key::PublicKey;

use crate::error::DsError;
use crate::opcua_sec::SecurityPolicy;

const OPEN_REQUEST: u32 = 446;
const OPEN_RESPONSE: u32 = 449;

#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key: RsaPrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelKeys {
    pub channel_id: u32,
    pub token_id: u32,
    pub policy: SecurityPolicy,
    pub client_sign: Vec<u8>,
    pub client_enc: Vec<u8>,
    pub client_iv: Vec<u8>,
    pub server_sign: Vec<u8>,
    pub server_enc: Vec<u8>,
    pub server_iv: Vec<u8>,
}

pub fn identity_named(name: &str) -> Result<Identity, DsError> {
    let key = RsaPrivateKey::new(&mut OsRng, 2048)
        .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let pkcs8 = key
        .to_pkcs8_der()
        .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let key_pair =
        rcgen::KeyPair::from_der_and_sign_algo(pkcs8.as_bytes(), &rcgen::PKCS_RSA_SHA256)
            .map_err(|err| DsError::new(format!("certificate key: {err}")))?;
    let mut params = rcgen::CertificateParams::new(vec![name.to_string()]);
    params.alg = &rcgen::PKCS_RSA_SHA256;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, name);
    params.key_pair = Some(key_pair);
    let cert = rcgen::Certificate::from_params(params)
        .map_err(|err| DsError::new(format!("certificate: {err}")))?;
    let cert_der = cert
        .serialize_der()
        .map_err(|err| DsError::new(format!("certificate der: {err}")))?;
    Ok(Identity { cert_der, key })
}

pub fn thumbprint(der: &[u8]) -> [u8; 20] {
    let digest = Sha1::digest(der);
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest);
    out
}

pub fn public_from_cert(der: &[u8]) -> Result<RsaPublicKey, DsError> {
    let (_, cert) = X509Certificate::from_der(der)
        .map_err(|_| DsError::new("opc ua certificate did not parse"))?;
    let key = cert
        .public_key()
        .parsed()
        .map_err(|_| DsError::new("opc ua certificate public key"))?;
    let PublicKey::RSA(rsa) = key else {
        return Err(DsError::new("opc ua certificate is not RSA"));
    };
    RsaPublicKey::new(
        rsa::BigUint::from_bytes_be(rsa.modulus),
        rsa::BigUint::from_bytes_be(rsa.exponent),
    )
    .map_err(|_| DsError::new("opc ua RSA public key"))
}

pub fn client_open(
    policy: SecurityPolicy,
    client: &Identity,
    server_cert: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, DsError> {
    let body = open_request_body(nonce);
    let server_public = public_from_cert(server_cert)?;
    seal_opn(
        policy,
        &client.key,
        &client.cert_der,
        &thumbprint(server_cert),
        0,
        &body,
        &server_public,
    )
}

pub fn server_accept(
    policy: SecurityPolicy,
    server: &Identity,
    message: &[u8],
    server_nonce: &[u8],
) -> Result<(Vec<u8>, ChannelKeys), DsError> {
    let (channel, client_cert, plain) = open_opn(policy, &server.key, message)?;
    if channel != 0 {
        return Err(DsError::new("open channel id is not zero"));
    }
    let client_nonce = nonce_after_security_mode(&plain)?;
    let client_public = public_from_cert(&client_cert)?;
    let channel_id = 7u32;
    let token_id = 3u32;
    let body = open_response_body(channel_id, token_id, server_nonce);
    let reply = seal_opn(
        policy,
        &server.key,
        &server.cert_der,
        &thumbprint(&client_cert),
        channel_id,
        &body,
        &client_public,
    )?;
    let keys = derive_keys(policy, &client_nonce, server_nonce, channel_id, token_id);
    Ok((reply, keys))
}

pub fn client_finish(
    policy: SecurityPolicy,
    client: &Identity,
    response: &[u8],
    client_nonce: &[u8],
) -> Result<ChannelKeys, DsError> {
    let (_channel, _cert, plain) = open_opn(policy, &client.key, response)?;
    let (channel_id, token_id, server_nonce) = parse_open_response(&plain)?;
    Ok(derive_keys(
        policy,
        client_nonce,
        &server_nonce,
        channel_id,
        token_id,
    ))
}

fn derive_keys(
    policy: SecurityPolicy,
    client_nonce: &[u8],
    server_nonce: &[u8],
    channel_id: u32,
    token_id: u32,
) -> ChannelKeys {
    let len = policy.derived_len();
    let client = p_hash(policy, server_nonce, client_nonce, len);
    let server = p_hash(policy, client_nonce, server_nonce, len);
    let split = |bytes: &[u8]| {
        let sign = policy.sign_key_len();
        let enc = policy.enc_key_len();
        (
            bytes[..sign].to_vec(),
            bytes[sign..sign + enc].to_vec(),
            bytes[sign + enc..sign + enc + policy.iv_len()].to_vec(),
        )
    };
    let (client_sign, client_enc, client_iv) = split(&client);
    let (server_sign, server_enc, server_iv) = split(&server);
    ChannelKeys {
        channel_id,
        token_id,
        policy,
        client_sign,
        client_enc,
        client_iv,
        server_sign,
        server_enc,
        server_iv,
    }
}

fn p_hash(policy: SecurityPolicy, secret: &[u8], seed: &[u8], len: usize) -> Vec<u8> {
    if policy.sym_sha256() {
        p_sha256(secret, seed, len)
    } else {
        p_sha1(secret, seed, len)
    }
}

fn p_sha1(secret: &[u8], seed: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut a = hmac_sha1(secret, seed);
    while out.len() < len {
        let mut data = a.clone();
        data.extend_from_slice(seed);
        out.extend_from_slice(&hmac_sha1(secret, &data));
        a = hmac_sha1(secret, &a);
    }
    out.truncate(len);
    out
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<Sha1>>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// TLS 1.2 P_SHA256, the construction OPC UA Part 6 uses for key derivation.
pub fn p_sha256(secret: &[u8], seed: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut a = hmac_sha256(secret, seed);
    while out.len() < len {
        let mut data = a.clone();
        data.extend_from_slice(seed);
        out.extend_from_slice(&hmac_sha256(secret, &data));
        a = hmac_sha256(secret, &a);
    }
    out.truncate(len);
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn open_request_body(nonce: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write_node_id(&mut body, OPEN_REQUEST);
    write_request_header(&mut body);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&3u32.to_le_bytes());
    write_bytes(&mut body, nonce);
    body.extend_from_slice(&3_600_000u32.to_le_bytes());
    body
}

fn open_response_body(channel_id: u32, token_id: u32, nonce: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write_node_id(&mut body, OPEN_RESPONSE);
    body.extend_from_slice(&0i64.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.push(0);
    body.extend_from_slice(&0i32.to_le_bytes());
    write_node_id(&mut body, 0);
    body.push(0);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&channel_id.to_le_bytes());
    body.extend_from_slice(&token_id.to_le_bytes());
    body.extend_from_slice(&0i64.to_le_bytes());
    body.extend_from_slice(&60_000u32.to_le_bytes());
    write_bytes(&mut body, nonce);
    body
}

fn parse_open_response(plain: &[u8]) -> Result<(u32, u32, Vec<u8>), DsError> {
    let nonce = last_byte_string(plain)?;
    let nonce_at = plain.len() - nonce.len() - 4;
    if nonce_at < 16 {
        return Err(DsError::new("open response token"));
    }
    let token_at = nonce_at - 4 - 8 - 4;
    let channel_at = token_at - 4;
    let channel_id = u32::from_le_bytes(
        plain[channel_at..channel_at + 4]
            .try_into()
            .unwrap_or([0; 4]),
    );
    let token_id = u32::from_le_bytes(plain[token_at..token_at + 4].try_into().unwrap_or([0; 4]));
    Ok((channel_id, token_id, nonce))
}

fn last_byte_string(bytes: &[u8]) -> Result<Vec<u8>, DsError> {
    for size in [32usize, 16, 48] {
        if bytes.len() >= 4 + size {
            let at = bytes.len() - size - 4;
            let n = i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or([0; 4]));
            if n == size as i32 {
                return Ok(bytes[at + 4..].to_vec());
            }
        }
    }
    Err(DsError::new("open response nonce"))
}

fn nonce_after_security_mode(bytes: &[u8]) -> Result<Vec<u8>, DsError> {
    // Client nonce is the byte string written after SecurityMode = 3.
    let marker = 3u32.to_le_bytes();
    let Some(at) = bytes.windows(4).position(|window| window == marker) else {
        return Err(DsError::new("security mode missing"));
    };
    let rest = &bytes[at + 4..];
    let (len, rest) = take_i32(rest)?;
    if len < 0 || rest.len() < len as usize {
        return Err(DsError::new("client nonce"));
    }
    Ok(rest[..len as usize].to_vec())
}

fn seal_opn(
    policy: SecurityPolicy,
    key: &RsaPrivateKey,
    cert: &[u8],
    receiver_thumb: &[u8; 20],
    channel_id: u32,
    body: &[u8],
    peer: &RsaPublicKey,
) -> Result<Vec<u8>, DsError> {
    let mut secret = Vec::new();
    secret.extend_from_slice(&1u32.to_le_bytes());
    secret.extend_from_slice(&1u32.to_le_bytes());
    secret.extend_from_slice(body);
    let padded = sign_and_pad(policy, key, peer, &secret)?;
    let cipher = rsa_encrypt(policy, peer, &padded)?;
    let mut header = b"OPNF".to_vec();
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&channel_id.to_le_bytes());
    write_string(&mut header, policy.uri());
    write_bytes(&mut header, cert);
    write_bytes(&mut header, receiver_thumb);
    header.extend_from_slice(&cipher);
    let size = header.len() as u32;
    header[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(header)
}

fn open_opn(
    policy: SecurityPolicy,
    key: &RsaPrivateKey,
    message: &[u8],
) -> Result<(u32, Vec<u8>, Vec<u8>), DsError> {
    if message.len() < 12 || &message[..4] != b"OPNF" {
        return Err(DsError::new("not an OPN chunk"));
    }
    let size = u32::from_le_bytes(message[4..8].try_into().unwrap_or([0; 4])) as usize;
    if size != message.len() {
        return Err(DsError::new("OPN size mismatch"));
    }
    let channel = u32::from_le_bytes(message[8..12].try_into().unwrap_or([0; 4]));
    let (uri, rest) = take_opc_string(&message[12..])?;
    if uri != policy.uri() {
        return Err(DsError::new(format!("OPN policy is not {}", policy.uri())));
    }
    let (cert_len, rest) = take_i32(rest)?;
    if cert_len < 0 || rest.len() < cert_len as usize {
        return Err(DsError::new("OPN sender certificate"));
    }
    let cert = &rest[..cert_len as usize];
    let sender = public_from_cert(cert)?;
    let rest = &rest[cert_len as usize..];
    let (thumb_len, rest) = take_i32(rest)?;
    if thumb_len != 20 || rest.len() < 20 {
        return Err(DsError::new("OPN receiver thumbprint"));
    }
    let cipher = &rest[20..];
    let plain = rsa_decrypt(policy, key, cipher)?;
    let content = verify_and_unpad(policy, &sender, &plain)?;
    if content.len() < 8 {
        return Err(DsError::new("OPN sequence header"));
    }
    Ok((channel, cert.to_vec(), content[8..].to_vec()))
}

fn modulus_len(public: &RsaPublicKey) -> usize {
    public.size()
}

fn sign_and_pad(
    policy: SecurityPolicy,
    key: &RsaPrivateKey,
    peer: &RsaPublicKey,
    data: &[u8],
) -> Result<Vec<u8>, DsError> {
    let modulus = modulus_len(peer);
    let plain_block = modulus - policy.rsa_overhead();
    let raw = data.len() + modulus;
    let pad = plain_block - (raw % plain_block);
    let mut signed = data.to_vec();
    signed.extend(std::iter::repeat_n((pad - 1) as u8, pad));
    let signature = sign_rsa(policy, key, &signed)?;
    signed.extend_from_slice(&signature);
    if signed.len() % plain_block != 0 {
        return Err(DsError::new("OPN padding is not a whole RSA block"));
    }
    Ok(signed)
}

fn verify_and_unpad(
    policy: SecurityPolicy,
    public: &RsaPublicKey,
    plain: &[u8],
) -> Result<Vec<u8>, DsError> {
    let modulus = modulus_len(public);
    if plain.len() < modulus + 1 {
        return Err(DsError::new("OPN plaintext is short"));
    }
    let (content, sig) = plain.split_at(plain.len() - modulus);
    verify_rsa(policy, public, content, sig)?;
    let mark = *content.last().unwrap_or(&0);
    let pad = mark as usize + 1;
    if pad > content.len() || content[content.len() - pad..].iter().any(|b| *b != mark) {
        return Err(DsError::new("OPN padding"));
    }
    Ok(content[..content.len() - pad].to_vec())
}

fn sign_rsa(policy: SecurityPolicy, key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>, DsError> {
    if policy.asym_sha256() {
        let signing = SigningKey::<Sha256>::new(key.clone());
        Ok(signing.sign(data).to_bytes().to_vec())
    } else {
        let signing = SigningKey::<Sha1>::new(key.clone());
        Ok(signing.sign(data).to_bytes().to_vec())
    }
}

fn verify_rsa(
    policy: SecurityPolicy,
    public: &RsaPublicKey,
    data: &[u8],
    sig: &[u8],
) -> Result<(), DsError> {
    let signature = Signature::try_from(sig).map_err(|_| DsError::new("OPN signature length"))?;
    if policy.asym_sha256() {
        VerifyingKey::<Sha256>::new(public.clone())
            .verify(data, &signature)
            .map_err(|_| DsError::new("OPN signature mismatch"))
    } else {
        VerifyingKey::<Sha1>::new(public.clone())
            .verify(data, &signature)
            .map_err(|_| DsError::new("OPN signature mismatch"))
    }
}

fn rsa_encrypt(
    policy: SecurityPolicy,
    public: &RsaPublicKey,
    data: &[u8],
) -> Result<Vec<u8>, DsError> {
    let plain_block = modulus_len(public) - policy.rsa_overhead();
    let mut out = Vec::new();
    for chunk in data.chunks(plain_block) {
        let block = if policy.oaep() {
            public
                .encrypt(&mut OsRng, Oaep::new::<Sha1>(), chunk)
                .map_err(|_| DsError::new("OPN RSA-OAEP encrypt"))?
        } else {
            public
                .encrypt(&mut OsRng, Pkcs1v15Encrypt, chunk)
                .map_err(|_| DsError::new("OPN RSA-15 encrypt"))?
        };
        out.extend_from_slice(&block);
    }
    Ok(out)
}

fn rsa_decrypt(
    policy: SecurityPolicy,
    key: &RsaPrivateKey,
    data: &[u8],
) -> Result<Vec<u8>, DsError> {
    let modulus = key.size();
    if modulus == 0 || data.len() % modulus != 0 {
        return Err(DsError::new("OPN ciphertext is not a whole RSA block"));
    }
    let mut out = Vec::new();
    for chunk in data.chunks(modulus) {
        let block = if policy.oaep() {
            key.decrypt(Oaep::new::<Sha1>(), chunk)
                .map_err(|_| DsError::new("OPN RSA-OAEP decrypt"))?
        } else {
            key.decrypt(Pkcs1v15Encrypt, chunk)
                .map_err(|_| DsError::new("OPN RSA-15 decrypt"))?
        };
        out.extend_from_slice(&block);
    }
    Ok(out)
}

fn write_node_id(out: &mut Vec<u8>, id: u32) {
    out.push(0x01);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
}

fn write_request_header(out: &mut Vec<u8>) {
    write_node_id(out, 0);
    out.extend_from_slice(&0i64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(-1i32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    write_node_id(out, 0);
    out.push(0);
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    write_bytes(out, text.as_bytes());
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn take_i32(bytes: &[u8]) -> Result<(i32, &[u8]), DsError> {
    if bytes.len() < 4 {
        return Err(DsError::new("opc ua integer"));
    }
    let value = i32::from_le_bytes(bytes[..4].try_into().unwrap_or([0; 4]));
    Ok((value, &bytes[4..]))
}

fn take_opc_string(bytes: &[u8]) -> Result<(String, &[u8]), DsError> {
    let (len, rest) = take_i32(bytes)?;
    if len < 0 {
        return Ok((String::new(), rest));
    }
    if rest.len() < len as usize {
        return Err(DsError::new("opc ua string"));
    }
    let text = std::str::from_utf8(&rest[..len as usize])
        .map_err(|_| DsError::new("opc ua string utf-8"))?
        .to_string();
    Ok((text, &rest[len as usize..]))
}
