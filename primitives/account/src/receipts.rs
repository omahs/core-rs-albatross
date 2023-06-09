use std::{fmt::Debug, io};

use nimiq_database_value::{FromDatabaseValue, IntoDatabaseValue};
use nimiq_primitives::account::FailReason;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountReceipt(pub Vec<u8>);

impl From<Vec<u8>> for AccountReceipt {
    fn from(val: Vec<u8>) -> Self {
        AccountReceipt(val)
    }
}

#[macro_export]
macro_rules! convert_receipt {
    ($t: ty) => {
        impl TryFrom<AccountReceipt> for $t {
            type Error = AccountError;

            fn try_from(value: AccountReceipt) -> Result<Self, Self::Error> {
                <$t>::try_from(&value)
            }
        }

        impl TryFrom<&AccountReceipt> for $t {
            type Error = AccountError;

            fn try_from(value: &AccountReceipt) -> Result<Self, Self::Error> {
                postcard::from_bytes(&value.0[..])
                    .map_err(|e| AccountError::InvalidSerialization(e))
            }
        }

        impl From<$t> for AccountReceipt {
            fn from(value: $t) -> Self {
                AccountReceipt::from(postcard::to_allocvec(&value).unwrap())
            }
        }
    };
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub sender_receipt: Option<AccountReceipt>,
    pub recipient_receipt: Option<AccountReceipt>,
    pub pruned_account: Option<AccountReceipt>,
}

pub type InherentReceipt = Option<AccountReceipt>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "T: Clone + Debug + Serialize + DeserializeOwned")]
#[repr(u8)]
pub enum OperationReceipt<T: Clone + Debug + Serialize + DeserializeOwned> {
    Ok(T),
    Err(T, FailReason),
}

pub type TransactionOperationReceipt = OperationReceipt<TransactionReceipt>;
pub type InherentOperationReceipt = OperationReceipt<InherentReceipt>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipts {
    pub transactions: Vec<TransactionOperationReceipt>,
    pub inherents: Vec<InherentOperationReceipt>,
}

// TODO Implement sparse serialization for Receipts

impl IntoDatabaseValue for Receipts {
    fn database_byte_size(&self) -> usize {
        postcard::to_allocvec(self).unwrap().len()
    }

    fn copy_into_database(&self, bytes: &mut [u8]) {
        postcard::to_slice(self, bytes).unwrap();
    }
}

impl FromDatabaseValue for Receipts {
    fn copy_from_database(bytes: &[u8]) -> io::Result<Self>
    where
        Self: Sized,
    {
        postcard::from_bytes(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
