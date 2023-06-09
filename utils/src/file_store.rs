use std::{
    fs::OpenOptions,
    io::{BufReader, BufWriter, Error as IoError, Write},
    path::{Path, PathBuf},
};

use thiserror::Error;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        FileStore {
            path: path.as_ref().to_owned(),
        }
    }

    pub fn load<'de, T: Deserialize<'de>, F>(&self) -> Result<T, Error<'de, F>>
    where
        F: Serializer + Deserializer<'de>,
    {
        log::debug!("Reading from: {}", self.path.display());
        let file = OpenOptions::new().read(true).open(&self.path)?;
        let mut buf_reader = BufReader::new(file);
        let item = Deserialize::deserialize(&mut buf_reader)?;
        Ok(item)
    }

    pub fn load_or_store<'de, T, F, G>(&self, mut f: F) -> Result<T, Error<'de, G>>
    where
        T: Serialize + Deserialize<'de>,
        F: FnMut() -> T,
        G: Serializer + Deserializer<'de>,
    {
        if self.path.exists() {
            self.load()
        } else {
            let x = f();
            self.store(&x)?;
            Ok(x)
        }
    }

    pub fn store<'de, T: Serialize, F>(&self, item: &T) -> Result<(), Error<'de, F>>
    where
        F: Serializer + Deserializer<'de>,
    {
        log::debug!("Writing to: {}", self.path.display());
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
pub enum Error<'de, T>
where
    T: Serializer + Deserializer<'de>,
{
    #[error("Serialization error: {0}")]
    Serialization(<T as Serializer>::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(<T as Deserializer<'de>>::Error),

    #[error("IO error: {0}")]
    IoError(#[from] IoError),
}
