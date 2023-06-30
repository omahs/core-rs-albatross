use std::{
    fs::OpenOptions,
    io::{BufWriter, Error as IoError, Read as _, Write},
    path::{Path, PathBuf},
};

use nimiq_serde::{Deserialize, Serialize, DeserializeError};
use thiserror::Error;

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        FileStore {
            path: path.as_ref().to_owned(),
        }
    }

    pub fn load<T: Deserialize>(&self) -> Result<T, Error> {
        log::debug!("Reading from: {}", self.path.display());
        let mut file = OpenOptions::new().read(true).open(&self.path)?;
        let mut buffer = Vec::with_capacity(4000);
        file.read_to_end(&mut buffer)?;
        let item: T = Deserialize::deserialize_from_vec(&buffer)?;
        Ok(item)
    }

    pub fn load_or_store<T, F>(&self, mut f: F) -> Result<T, Error>
    where
        T: Serialize + Deserialize,
        F: FnMut() -> T,
    {
        if self.path.exists() {
            self.load()
        } else {
            let x = f();
            self.store(&x)?;
            Ok(x)
        }
    }

    pub fn store<T: Serialize>(&self, item: &T) -> Result<(), Error> {
        log::debug!(path = ?self.path.display(), "Writing tof file");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.path)?;
        let mut buf_writer = BufWriter::new(file);
        Serialize::serialize(item, &mut buf_writer)?;
        buf_writer.flush()?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Serialization error: {0}")]
    Serialization(#[from] DeserializeError),

    #[error("IO error: {0}")]
    IoError(#[from] IoError),
}
