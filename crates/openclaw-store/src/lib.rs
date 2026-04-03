mod conversation;

use std::path::Path;

use openclaw_core::Error;

pub use conversation::ConversationStore;

/// Top-level database handle wrapping a sled instance.
#[derive(Clone)]
pub struct Store {
    db: sled::Db,
}

impl Store {
    /// Open (or create) a store at the given directory path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let db = sled::open(path).map_err(|e| Error::Storage(e.to_string()))?;
        Ok(Self { db })
    }

    /// Return a handle for conversation operations.
    pub fn conversations(&self) -> ConversationStore {
        let tree = self
            .db
            .open_tree("conversations")
            .expect("failed to open conversations tree");
        ConversationStore::new(tree)
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<(), Error> {
        self.db
            .flush()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }
}
