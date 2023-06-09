use std::{
    fs::OpenOptions,
    io::{BufWriter, Error as IoError, Read, Write},
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};
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

    pub fn load<T: DeserializeOwned>(&self) -> Result<T, Error> {
        log::debug!("Reading from: {}", self.path.display());
        let mut file = OpenOptions::new().read(true).open(&self.path)?;
        let mut buffer = Vec::with_capacity(4000);
        file.read_to_end(&mut buffer)?;
        let item: T = postcard::from_bytes(&buffer)?;
        Ok(item)
    }

    pub fn load_or_store<T, F>(&self, mut f: F) -> Result<T, Error>
    where
        T: Serialize + DeserializeOwned + Clone,
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
        buf_writer.write_all(&postcard::to_allocvec(item)?)?;
        buf_writer.flush()?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Serialization error: {0}")]
    Serialization(#[from] postcard::Error),

    #[error("IO error: {0}")]
    IoError(#[from] IoError),
}
