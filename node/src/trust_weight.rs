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
