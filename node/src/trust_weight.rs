use std::collections::HashMap;

pub struct TrustWeightStore {
    weights: HashMap<String, f64>,
    tree: Option<sled::Tree>,
}

impl TrustWeightStore {
    pub fn new() -> Self {
        Self {
            weights: HashMap::new(),
            tree: None,
        }
    }

    pub fn with_persistence(db: &sled::Db) -> Self {
        let tree = db.open_tree("trust_weights").ok();
        let mut store = Self {
            weights: HashMap::new(),
            tree,
        };
        store.load_from_disk();
        store
    }

    fn load_from_disk(&mut self) {
        let Some(tree) = &self.tree else { return };
        for entry in tree.iter() {
            let Ok((key_bytes, weight_bytes)) = entry else {
                continue;
            };
            if weight_bytes.len() == 8 {
                if let Ok(key) = std::str::from_utf8(&key_bytes) {
                    let weight =
                        f64::from_le_bytes(weight_bytes.as_ref().try_into().unwrap());
                    self.weights.insert(key.to_string(), weight);
                }
            }
        }
    }

    pub fn set(&mut self, address: &str, weight: f64) {
        self.weights.insert(address.to_string(), weight);
        if let Some(tree) = &self.tree {
            let _ = tree.insert(address.as_bytes(), &weight.to_le_bytes());
        }
    }

    pub fn get(&self, address: &str) -> Option<f64> {
        self.weights.get(address).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── in-memory (no persistence) ─────────────────────────────────────────

    #[test]
    fn set_and_get_round_trip() {
        let mut store = TrustWeightStore::new();
        store.set("addr1", 0.75);
        assert_eq!(store.get("addr1"), Some(0.75));
    }

    #[test]
    fn missing_key_returns_none() {
        let store = TrustWeightStore::new();
        assert_eq!(store.get("nonexistent"), None);
    }

    #[test]
    fn overwrite_updates_value() {
        let mut store = TrustWeightStore::new();
        store.set("addr1", 0.5);
        store.set("addr1", 0.9);
        assert_eq!(store.get("addr1"), Some(0.9));
    }

    #[test]
    fn multiple_distinct_keys() {
        let mut store = TrustWeightStore::new();
        store.set("a", 0.1);
        store.set("b", 0.2);
        store.set("c", 0.3);
        assert_eq!(store.get("a"), Some(0.1));
        assert_eq!(store.get("b"), Some(0.2));
        assert_eq!(store.get("c"), Some(0.3));
    }

    #[test]
    fn weight_zero_stored_and_retrieved() {
        let mut store = TrustWeightStore::new();
        store.set("addr_zero", 0.0);
        assert_eq!(store.get("addr_zero"), Some(0.0));
    }

    #[test]
    fn weight_one_stored_and_retrieved() {
        let mut store = TrustWeightStore::new();
        store.set("addr_max", 1.0);
        assert_eq!(store.get("addr_max"), Some(1.0));
    }

    // ── persistence ────────────────────────────────────────────────────────

    #[test]
    fn persistence_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("trust.sled");

        // Write in first instance
        {
            let db = sled::open(&db_path).unwrap();
            let mut store = TrustWeightStore::with_persistence(&db);
            store.set("persistent_addr", 0.42);
        }

        // Re-open and verify data was loaded from disk
        {
            let db = sled::open(&db_path).unwrap();
            let store = TrustWeightStore::with_persistence(&db);
            let weight = store.get("persistent_addr");
            assert!(
                weight.is_some(),
                "weight should be present after reload from sled"
            );
            let w = weight.unwrap();
            assert!(
                (w - 0.42).abs() < 1e-12,
                "reloaded weight {w} differs from stored 0.42"
            );
        }
    }

    #[test]
    fn persistence_overwrite_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("overwrite.sled");

        {
            let db = sled::open(&db_path).unwrap();
            let mut store = TrustWeightStore::with_persistence(&db);
            store.set("addr", 0.1);
            store.set("addr", 0.99);
        }

        {
            let db = sled::open(&db_path).unwrap();
            let store = TrustWeightStore::with_persistence(&db);
            let w = store.get("addr").expect("key must survive reload");
            assert!(
                (w - 0.99).abs() < 1e-12,
                "expected 0.99 after overwrite, got {w}"
            );
        }
    }

    #[test]
    fn in_memory_and_persistent_coexist_independently() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("coexist.sled");
        let db = sled::open(&db_path).unwrap();

        let mut mem_store = TrustWeightStore::new();
        let mut pers_store = TrustWeightStore::with_persistence(&db);

        mem_store.set("addr", 0.3);
        pers_store.set("addr", 0.7);

        // Each store sees its own value
        assert_eq!(mem_store.get("addr"), Some(0.3));
        assert_eq!(pers_store.get("addr"), Some(0.7));
    }
}
