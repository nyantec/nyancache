mod base32;

use crate::error::Error;
use log::warn;
use ring::signature;
use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use strum_macros::{AsRefStr, EnumString};

#[derive(Debug, Clone)]
pub struct Signature {
    pub key_name: String,
    pub signature: Vec<u8>,
}

impl FromStr for Signature {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Signature, Self::Err> {
        let mut parts = s.splitn(2, ":");
        let sig = Signature {
            key_name: parts.next().ok_or(Error::UnexpectedEof)?.to_string(),
            signature: base64::decode(parts.next().ok_or(Error::UnexpectedEof)?.as_bytes())?,
        };
        Ok(sig)
    }
}

impl std::fmt::Display for Signature {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}:{}", self.key_name, base64::encode(&self.signature))
    }
}

#[derive(Debug, Clone)]
pub struct PubKey {
    pub key_name: String,
    pub pub_key: Vec<u8>,
}

impl FromStr for PubKey {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<PubKey, Self::Err> {
        let mut parts = s.splitn(2, ":");
        Ok(PubKey {
            key_name: parts.next().ok_or(Error::UnexpectedEof)?.to_string(),
            pub_key: base64::decode(parts.next().ok_or(Error::UnexpectedEof)?.as_bytes())?,
        })
    }
}

impl std::fmt::Display for PubKey {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}:{}", self.key_name, base64::encode(&self.pub_key))
    }
}

#[derive(AsRefStr, EnumString, PartialEq, Debug, Clone)]
pub enum HashType {
    #[strum(serialize = "md5")]
    Md5,
    #[strum(serialize = "sha1")]
    Sha1,
    #[strum(serialize = "sha256")]
    Sha256,
    #[strum(serialize = "sha512")]
    Sha512,
}

#[derive(Debug, Clone)]
pub struct NixHash {
    hash_type: HashType,
    hash: Vec<u8>,
}

impl FromStr for NixHash {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<NixHash, Self::Err> {
        let mut parts = s.splitn(2, ":");
        Ok(NixHash {
            hash_type: HashType::from_str(parts.next().ok_or(Error::UnexpectedEof)?)
                .map_err(|_| Error::UnknownHashType)?,
            hash: base32::decode(parts.next().ok_or(Error::UnexpectedEof)?)?,
        })
    }
}

impl std::fmt::Display for NixHash {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            fmt,
            "{}:{}",
            self.hash_type.as_ref(),
            base32::encode(&self.hash)
        )
    }
}

#[derive(AsRefStr, EnumString, PartialEq, Debug, Clone)]
pub enum Compression {
    #[strum(serialize = "xz")]
    Xz,
    #[strum(serialize = "bzip2")]
    Bzip2,
    #[strum(serialize = "gzip")]
    Gzip,
    #[strum(serialize = "zstd")]
    Zstd,
    #[strum(serialize = "none")]
    Plain,
}

#[derive(Debug, Clone)]
pub struct NarInfo {
    pub path: String,
    pub nar_hash: NixHash,
    pub nar_size: u64,
    pub file_hash: Option<NixHash>,
    pub file_size: Option<u64>,
    pub url: Option<String>,
    pub compression: Option<Compression>,
    pub deriver: Option<String>,
    pub ca: Option<String>,
    pub references: BTreeSet<String>,
    pub signatures: HashMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub struct SignatureVerified;

impl NarInfo {
    fn fingerprint(&self) -> String {
        format!(
            "1;{};{};{};{}",
            self.path,
            self.nar_hash,
            self.nar_size,
            self.references
                .clone()
                .into_iter()
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn check_signature(&self, trusted_keys: &Vec<PubKey>) -> Result<SignatureVerified, Error> {
        let fingerprint = self.fingerprint();
        for trusted_key in trusted_keys {
            if let Some(sig) = self.signatures.get(&trusted_key.key_name) {
                let peer_public_key = signature::UnparsedPublicKey::new(
                    &signature::ED25519,
                    trusted_key.pub_key.clone(),
                );
                if let Ok(()) = peer_public_key.verify(fingerprint.as_bytes(), &sig) {
                    return Ok(SignatureVerified);
                }
            }
        }
        return Err(Error::NoValidSignature);
    }
}

impl FromStr for NarInfo {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<NarInfo, Self::Err> {
        let mut path = None;
        let mut nar_hash = None;
        let mut nar_size = None;
        let mut file_hash = None;
        let mut file_size = None;
        let mut url = None;
        let mut compression = None;
        let mut deriver = None;
        let mut ca = None;
        let mut references = BTreeSet::new();
        let mut signatures = HashMap::new();

        for line in s.lines() {
            let colon = line.find(':').ok_or(Error::BadNarInfo)?;

            let (name, value) = line.split_at(colon);

            if !value.starts_with(": ") {
                return Err(Error::BadNarInfo);
            }

            let value = &value[2..];

            match name {
                "StorePath" => path = Some(value.into()),
                "NarHash" => nar_hash = Some(NixHash::from_str(value)?),
                "NarSize" => nar_size = Some(value.parse().map_err(|_| Error::BadNarInfo)?),
                "FileHash" => file_hash = Some(NixHash::from_str(value)?),
                "FileSize" => file_size = Some(value.parse().map_err(|_| Error::BadNarInfo)?),
                "URL" => url = Some(value.into()),
                "Compression" => {
                    compression = Some(Compression::from_str(value).map_err(|_| Error::BadNarInfo)?)
                }
                "Deriver" => deriver = Some(value.into()),
                "References" => {
                    for r in value.split(' ') {
                        references.insert(format!("/nix/store/{}", r));
                    }
                }
                "Sig" => {
                    let sig = Signature::from_str(value)?;
                    if let Some(_existing) = signatures.insert(sig.key_name, sig.signature) {
                        warn!("Duplicate signature");
                    }
                }
                "CA" => ca = Some(value.into()),
                _ => warn!("unknown key: {}\n{}", name, line),
            }
        }

        Ok(NarInfo {
            path: path.ok_or(Error::BadNarInfo)?,
            nar_hash: nar_hash.ok_or(Error::BadNarInfo)?,
            nar_size: nar_size.ok_or(Error::BadNarInfo)?,
            file_hash,
            file_size,
            url,
            compression,
            deriver,
            ca,
            references,
            signatures,
        })
    }
}

impl std::fmt::Display for NarInfo {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "StorePath: {}\n", self.path)?;
        write!(fmt, "NarHash: {}\n", self.nar_hash)?;
        write!(fmt, "NarSize: {}\n", self.nar_size)?;
        if let Some(file_hash) = self.file_hash.as_ref() {
            write!(fmt, "FileHash: {}\n", file_hash)?;
        }
        if let Some(file_size) = self.file_size {
            write!(fmt, "FileSize: {}\n", file_size)?;
        }
        if let Some(url) = self.url.as_ref() {
            write!(fmt, "URL: {}\n", url)?;
        }
        if let Some(compression) = self.compression.as_ref() {
            write!(fmt, "Compression: {}\n", (*compression).as_ref())?;
        }
        if let Some(deriver) = self.deriver.as_ref() {
            write!(fmt, "Deriver: {}\n", deriver)?;
        }
        if self.references.len() > 0 {
            write!(fmt, "References:")?;
            for reference in &self.references {
                if let Some(stripped) = reference.strip_prefix("/nix/store/") {
                    write!(fmt, " {}", stripped)?;
                } else {
                    warn!("invalid store prefix in saved narinfo");
                }
            }
            write!(fmt, "\n")?;
        }
        for sig in self.signatures.clone() {
            write!(fmt, "Sig: {}\n", Signature { key_name: sig.0, signature: sig.1 })?;
        }
        if let Some(ca) = self.ca.as_ref() {
            write!(fmt, "CA: {}\n", ca)?;
        }
        Ok(())
    }
}
