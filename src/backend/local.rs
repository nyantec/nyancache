use super::{Backend, NarResponder};
use tokio::io::BufWriter;
use tokio::fs;
use std::path::PathBuf;
use rocket::data::DataStream;
use crate::error::{Error, Result};

pub struct LocalBackend {
    tmp_dir: PathBuf,
    data_dir: PathBuf,
}

impl LocalBackend {
    pub fn new<T: Into<PathBuf>, U: Into<PathBuf>>(tmp_dir: T, data_dir: U) -> Self {
        Self {
            tmp_dir: tmp_dir.into(),
            data_dir: data_dir.into(),
        }
    }
    pub fn new_current_dir() -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let backend = Self::new(current_dir.join("tmp"), current_dir.join("data"));
        Ok(backend)
    }
}

#[async_trait::async_trait]
impl Backend for LocalBackend {
    async fn read_nar(&self, url: &str) -> Result<NarResponder> {
        let path = self.data_dir.join(url);
        let file = fs::File::open(&path).await?;
        Ok(NarResponder::File(file))
    }
    async fn write_nar(&self, url: &str, reader: &mut DataStream<'_>) -> Result<()> {
        let path = self.tmp_dir.join(url);
        fs::create_dir_all(&path.parent().ok_or(Error::Upload)?).await?;
        let mut file = fs::File::create(&path).await?;
        tokio::io::copy(reader, &mut BufWriter::new(&mut file)).await?;
        Ok(())
    }
    async fn finish_nar(&self, url: &str) -> Result<()> {
        let tmppath = self.tmp_dir.join(url);
        let newpath = self.data_dir.join(tmppath.strip_prefix(&self.tmp_dir).map_err(|_| Error::Upload)?);
        fs::create_dir_all(&newpath.parent().ok_or(Error::Upload)?).await?;
        fs::rename(&tmppath, newpath).await?;
        Ok(())
    }
}
