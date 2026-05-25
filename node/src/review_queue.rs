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

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> (sled::Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = sled::open(dir.path().join("review.sled")).unwrap();
        (db, dir)
    }

    #[test]
    fn submit_and_list_pending() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr1", "suspicious activity".into(), 0.3).unwrap();

        let pending = queue.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].address, "addr1");
        assert_eq!(pending[0].publisher_reason, "suspicious activity");
        assert!((pending[0].current_weight - 0.3).abs() < 1e-12);
    }

    #[test]
    fn resolve_removes_entry() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_remove", "test reason".into(), 0.5).unwrap();
        assert_eq!(queue.list_pending().len(), 1);

        let removed = queue.resolve("addr_remove").unwrap();
        assert!(removed, "resolve should return true when entry existed");
        assert_eq!(queue.list_pending().len(), 0);
    }

    #[test]
    fn resolve_nonexistent_returns_false() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        let removed = queue.resolve("addr_not_there").unwrap();
        assert!(!removed, "resolve of nonexistent entry must return false");
    }

    #[test]
    fn multiple_entries_all_listed() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_a", "reason A".into(), 0.1).unwrap();
        queue.submit("addr_b", "reason B".into(), 0.2).unwrap();
        queue.submit("addr_c", "reason C".into(), 0.3).unwrap();

        let pending = queue.list_pending();
        assert_eq!(pending.len(), 3);

        let addresses: Vec<&str> = pending.iter().map(|e| e.address.as_str()).collect();
        assert!(addresses.contains(&"addr_a"));
        assert!(addresses.contains(&"addr_b"));
        assert!(addresses.contains(&"addr_c"));
    }

    #[test]
    fn resolve_only_removes_target_entry() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_x", "reason X".into(), 0.6).unwrap();
        queue.submit("addr_y", "reason Y".into(), 0.7).unwrap();

        queue.resolve("addr_x").unwrap();

        let pending = queue.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].address, "addr_y");
    }

    #[test]
    fn submit_overwrites_existing_entry() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_dup", "first reason".into(), 0.4).unwrap();
        queue.submit("addr_dup", "updated reason".into(), 0.8).unwrap();

        let pending = queue.list_pending();
        // sled key is address, so second submit overwrites
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].publisher_reason, "updated reason");
        assert!((pending[0].current_weight - 0.8).abs() < 1e-12);
    }

    #[test]
    fn submitted_at_is_populated() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_ts", "ts check".into(), 0.5).unwrap();

        let pending = queue.list_pending();
        assert_eq!(pending.len(), 1);
        // submitted_at is an RFC3339 string, must not be empty
        assert!(!pending[0].submitted_at.is_empty());
    }

    #[test]
    fn resolve_twice_second_call_returns_false() {
        let (db, _dir) = open_db();
        let queue = ReviewQueue::new(&db);

        queue.submit("addr_twice", "reason".into(), 0.5).unwrap();

        let first = queue.resolve("addr_twice").unwrap();
        assert!(first);

        let second = queue.resolve("addr_twice").unwrap();
        assert!(!second, "second resolve of same address must return false");
    }
}
