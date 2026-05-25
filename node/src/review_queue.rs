use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewEntry {
    pub address: String,
    pub submitted_at: String,
    pub publisher_reason: String,
    pub current_weight: f64,
}

pub struct ReviewQueue {
    tree: sled::Tree,
}

impl ReviewQueue {
    pub fn new(db: &sled::Db) -> Self {
        Self {
            tree: db.open_tree("review_queue").expect("failed to open review_queue tree"),
        }
    }

    pub fn submit(&self, address: &str, reason: String, current_weight: f64) -> anyhow::Result<()> {
        let entry = ReviewEntry {
            address: address.to_string(),
            submitted_at: chrono::Utc::now().to_rfc3339(),
            publisher_reason: reason,
            current_weight,
        };
        let value = serde_json::to_vec(&entry)?;
        self.tree.insert(address.as_bytes(), value)?;
        Ok(())
    }

    pub fn list_pending(&self) -> Vec<ReviewEntry> {
        self.tree
            .iter()
            .filter_map(|entry| {
                let (_, value) = entry.ok()?;
                serde_json::from_slice(&value).ok()
            })
            .collect()
    }

    pub fn resolve(&self, address: &str) -> anyhow::Result<bool> {
        let removed = self.tree.remove(address.as_bytes())?.is_some();
        Ok(removed)
    }
}
