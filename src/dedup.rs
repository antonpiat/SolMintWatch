use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DedupStore {
    inner: Arc<Mutex<HashSet<String>>>,
}

impl DedupStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns true if the key was newly inserted (not a duplicate).
    pub async fn try_insert(&self, key: &str) -> bool {
        let mut guard = self.inner.lock().await;
        guard.insert(key.to_string())
    }
}

impl Default for DedupStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dedup_rejects_duplicates() {
        let store = DedupStore::new();
        assert!(store.try_insert("mint1").await);
        assert!(!store.try_insert("mint1").await);
        assert!(store.try_insert("mint2").await);
    }
}
