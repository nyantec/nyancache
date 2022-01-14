use std::str::FromStr;

use super::nixutils::{Compression, NarInfo, NixHash, Signature};
use super::schema::paths;

use diesel_derives::{Insertable, Queryable};
use serde::Serialize;

#[derive(Clone, Debug, Default, Queryable, Serialize, Insertable, Identifiable)]
#[table_name = "paths"]
#[primary_key("id")]
pub struct DbPath {
    pub id: String,
    path: String,
    registration_time: Option<i64>,
    last_accessed: Option<i64>,
    nar_size: i32,
    nar_hash: String,
    file_size: Option<i32>,
    file_hash: Option<String>,
    pub url: Option<String>,
    compression: Option<String>,
    deriver: Option<String>,
    ca: Option<String>,
    sigs: String,
    refs: String,
}

impl From<NarInfo> for DbPath {
    fn from(nar_info: NarInfo) -> Self {
        Self {
            id: "".to_string(),
            path: nar_info.path,
            registration_time: None,
            last_accessed: None,
            nar_size: nar_info.nar_size as i32,
            nar_hash: nar_info.nar_hash.to_string(),
            file_size: nar_info.file_size.map(|x| x as i32),
            file_hash: nar_info.file_hash.map(|x| x.to_string()),
            url: nar_info.url,
            compression: nar_info.compression.map(|x| x.as_ref().to_string()),
            deriver: nar_info.deriver,
            ca: nar_info.ca,
            sigs: nar_info
                .signatures
                .into_iter()
                .map(|(key_name, signature)| {
                    (Signature {
                        key_name,
                        signature,
                    })
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join(" "),
            refs: nar_info
                .references
                .into_iter()
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}
impl Into<NarInfo> for DbPath {
    fn into(self) -> NarInfo {
        NarInfo {
            path: self.path,
            nar_size: self.nar_size as u64,
            nar_hash: NixHash::from_str(&self.nar_hash).unwrap(),
            file_size: self.file_size.map(|x| x as u64),
            file_hash: self.file_hash.map(|x| NixHash::from_str(&x).unwrap()),
            url: self.url,
            compression: self.compression.map(|x| Compression::from_str(&x).unwrap()),
            deriver: self.deriver,
            ca: self.ca,
            signatures: self
                .sigs
                .split(" ")
                .map(|x| {
                    let sig = Signature::from_str(&x).unwrap();
                    (sig.key_name, sig.signature)
                })
                .collect(),
            references: self.refs.split(" ").map(|x| x.to_string()).collect(),
        }
    }
}
