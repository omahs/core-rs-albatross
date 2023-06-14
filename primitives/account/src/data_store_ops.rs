use nimiq_primitives::key_nibbles::KeyNibbles;
use serde::de::DeserializeOwned;

/// Read Operations that a Data Store needs to implement to interact with
/// contracts in the Accounts Trie.
pub trait DataStoreReadOps {
    fn get<T: DeserializeOwned>(&self, key: &KeyNibbles) -> Option<T>;
}

/// Expensive iteration operations that a Data Store can implement
/// for the Accounts Trie.
pub trait DataStoreIterOps {
    type Iter<T: DeserializeOwned>: Iterator<Item = T>;

    /// Returns an iterator over all items within a given range (inclusive).
    fn iter<T: DeserializeOwned>(
        &self,
        start_key: &KeyNibbles,
        end_key: &KeyNibbles,
    ) -> Self::Iter<T>;
}
