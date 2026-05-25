use libp2p::kad::store::{Error as KadStoreError, RecordStore, Result as KadStoreResult};
use libp2p::kad::{Record, RecordKey, ProviderRecord};
use p2s_card::{verify_card, verify_address};
use p2s_proto::{canonical_decode, SignedCard};
use std::borrow::Cow;
use std::collections::HashMap;

/// BEP44-style mutable record store for service cards.
///
/// Invariants enforced on every put:
/// - Card signature verifies against the embedded pubkey (invariant §1.2)
/// - Record key == BLAKE3(pubkey) (invariant §1.1)
/// - Higher seq wins; equal or lower seq is rejected
pub struct CardRecordStore {
    records: HashMap<RecordKey, Record>,
    seq_cache: HashMap<RecordKey, u64>,
    weight_cache: HashMap<RecordKey, f64>,
    max_records: usize,
    db: Option<sled::Db>,
}

impl CardRecordStore {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: HashMap::new(),
            seq_cache: HashMap::new(),
            weight_cache: HashMap::new(),
            max_records,
            db: None,
        }
    }

    pub fn with_persistence(max_records: usize, path: &std::path::Path) -> Self {
        let db_path = path.join("cards.sled");
        let db = sled::open(&db_path).ok();

        let mut store = Self {
            records: HashMap::new(),
            seq_cache: HashMap::new(),
            weight_cache: HashMap::new(),
            max_records,
            db,
        };
        store.load_from_disk();
        store
    }

    pub fn set_trust_weight(&mut self, key: &RecordKey, weight: f64) {
        self.weight_cache.insert(key.clone(), weight);
        if let Some(db) = &self.db {
            if let Ok(tree) = db.open_tree("weights") {
                let _ = tree.insert(key.as_ref(), &weight.to_le_bytes());
            }
        }
    }

    pub fn get_trust_weight(&self, key: &RecordKey) -> Option<f64> {
        self.weight_cache.get(key).copied()
    }

    fn load_from_disk(&mut self) {
        let Some(db) = &self.db else { return };
        let mut loaded = 0;
        for entry in db.iter() {
            let Ok((key_bytes, value_bytes)) = entry else { continue };
            let record = Record {
                key: RecordKey::new(&key_bytes),
                value: value_bytes.to_vec(),
                publisher: None,
                expires: None,
            };
            if let Ok(seq) = self.validate_and_extract_seq(&record) {
                let rk = record.key.clone();
                self.seq_cache.insert(rk.clone(), seq);
                self.records.insert(rk, record);
                loaded += 1;
            }
        }
        if loaded > 0 {
            tracing::info!(loaded, "Loaded records from disk");
        }

        if let Some(db) = &self.db {
            if let Ok(tree) = db.open_tree("weights") {
                for entry in tree.iter() {
                    let Ok((key_bytes, weight_bytes)) = entry else {
                        continue;
                    };
                    if weight_bytes.len() == 8 {
                        let weight = f64::from_le_bytes(weight_bytes.as_ref().try_into().unwrap());
                        self.weight_cache
                            .insert(RecordKey::new(&key_bytes), weight);
                    }
                }
            }
        }
    }

    fn persist(&self, record: &Record) {
        if let Some(db) = &self.db {
            let _ = db.insert(record.key.as_ref(), record.value.as_slice());
        }
    }

    fn unpersist(&self, key: &RecordKey) {
        if let Some(db) = &self.db {
            let _ = db.remove(key.as_ref());
        }
    }

    fn validate_and_extract_seq(&self, record: &Record) -> Result<u64, KadStoreError> {
        let signed_card: SignedCard = canonical_decode(&record.value)
            .map_err(|_| KadStoreError::ValueTooLarge)?;

        verify_card(&signed_card)
            .map_err(|_| KadStoreError::ValueTooLarge)?;

        let expected_address = p2s_card::compute_address(&signed_card.record.pubkey);
        verify_address(&signed_card, &expected_address)
            .map_err(|_| KadStoreError::ValueTooLarge)?;

        let key_bytes = record.key.as_ref();
        if key_bytes != expected_address {
            return Err(KadStoreError::ValueTooLarge);
        }

        Ok(signed_card.record.seq)
    }
}

impl RecordStore for CardRecordStore {
    type RecordsIter<'a> = std::vec::IntoIter<Cow<'a, Record>>;
    type ProvidedIter<'a> = std::vec::IntoIter<Cow<'a, ProviderRecord>>;

    fn get(&self, key: &RecordKey) -> Option<Cow<'_, Record>> {
        self.records.get(key).map(Cow::Borrowed)
    }

    fn put(&mut self, record: Record) -> KadStoreResult<()> {
        if self.records.len() >= self.max_records && !self.records.contains_key(&record.key) {
            return Err(KadStoreError::MaxRecords);
        }

        let new_seq = self.validate_and_extract_seq(&record)?;

        if let Some(&existing_seq) = self.seq_cache.get(&record.key) {
            if new_seq <= existing_seq {
                return Ok(());
            }
        }

        let key = record.key.clone();
        self.seq_cache.insert(key.clone(), new_seq);
        self.persist(&record);
        self.records.insert(key, record);
        Ok(())
    }

    fn remove(&mut self, key: &RecordKey) {
        self.records.remove(key);
        self.seq_cache.remove(key);
        self.unpersist(key);
    }

    fn records(&self) -> Self::RecordsIter<'_> {
        self.records.values().map(Cow::Borrowed).collect::<Vec<_>>().into_iter()
    }

    fn add_provider(&mut self, _record: ProviderRecord) -> KadStoreResult<()> {
        Ok(())
    }

    fn providers(&self, _key: &RecordKey) -> Vec<ProviderRecord> {
        vec![]
    }

    fn provided(&self) -> Self::ProvidedIter<'_> {
        vec![].into_iter()
    }

    fn remove_provider(&mut self, _key: &RecordKey, _provider: &libp2p::PeerId) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2s_card::{generate_keypair, sign_card, compute_address, CardRecord, CardStatus};
    use p2s_proto::canonical_encode;

    fn make_signed_record(seq: u64) -> (Record, p2s_card::CardKeypair) {
        let keypair = generate_keypair();
        let card = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq,
            status: CardStatus::Active,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: blake3::hash(b"manifest").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let signed = sign_card(card, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);
        let value = canonical_encode(&signed).unwrap();
        let record = Record {
            key: RecordKey::new(&address),
            value,
            publisher: None,
            expires: None,
        };
        (record, keypair)
    }

    fn make_signed_record_with_keypair(seq: u64, keypair: &p2s_card::CardKeypair) -> Record {
        let card = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq,
            status: CardStatus::Active,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: blake3::hash(b"manifest").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let signed = sign_card(card, &keypair.signing_key).unwrap();
        let address = compute_address(&signed.record.pubkey);
        let value = canonical_encode(&signed).unwrap();
        Record {
            key: RecordKey::new(&address),
            value,
            publisher: None,
            expires: None,
        }
    }

    #[test]
    fn put_and_get_valid_record() {
        let mut store = CardRecordStore::new(100);
        let (record, _) = make_signed_record(1);
        let key = record.key.clone();
        assert!(store.put(record).is_ok());
        assert!(store.get(&key).is_some());
    }

    #[test]
    fn reject_tampered_record() {
        let mut store = CardRecordStore::new(100);
        let (mut record, _) = make_signed_record(1);
        // Corrupt the value
        if let Some(byte) = record.value.last_mut() {
            *byte ^= 0xFF;
        }
        assert!(store.put(record).is_err());
    }

    #[test]
    fn reject_wrong_key() {
        let mut store = CardRecordStore::new(100);
        let (mut record, _) = make_signed_record(1);
        // Use a different key than BLAKE3(pubkey)
        record.key = RecordKey::new(&[0xFFu8; 32]);
        assert!(store.put(record).is_err());
    }

    #[test]
    fn higher_seq_supersedes() {
        let mut store = CardRecordStore::new(100);
        let (record_v1, keypair) = make_signed_record(1);
        let key = record_v1.key.clone();
        store.put(record_v1).unwrap();

        let record_v2 = make_signed_record_with_keypair(2, &keypair);
        store.put(record_v2).unwrap();

        let stored = store.get(&key).unwrap();
        let signed: SignedCard = canonical_decode(&stored.value).unwrap();
        assert_eq!(signed.record.seq, 2);
    }

    #[test]
    fn lower_seq_ignored() {
        let mut store = CardRecordStore::new(100);
        let (record_v2, keypair) = make_signed_record(2);
        let key = record_v2.key.clone();
        store.put(record_v2).unwrap();

        let record_v1 = make_signed_record_with_keypair(1, &keypair);
        store.put(record_v1).unwrap(); // should be silently ignored

        let stored = store.get(&key).unwrap();
        let signed: SignedCard = canonical_decode(&stored.value).unwrap();
        assert_eq!(signed.record.seq, 2);
    }

    #[test]
    fn equal_seq_ignored() {
        let mut store = CardRecordStore::new(100);
        let (record, keypair) = make_signed_record(5);
        let key = record.key.clone();
        store.put(record).unwrap();

        let same_seq = make_signed_record_with_keypair(5, &keypair);
        store.put(same_seq).unwrap(); // silently ignored

        assert!(store.get(&key).is_some());
    }

    #[test]
    fn max_records_enforced() {
        let mut store = CardRecordStore::new(1);
        let (r1, _) = make_signed_record(1);
        store.put(r1).unwrap();

        let (r2, _) = make_signed_record(1);
        assert!(matches!(store.put(r2), Err(KadStoreError::MaxRecords)));
    }

    #[test]
    fn remove_works() {
        let mut store = CardRecordStore::new(100);
        let (record, _) = make_signed_record(1);
        let key = record.key.clone();
        store.put(record).unwrap();
        store.remove(&key);
        assert!(store.get(&key).is_none());
    }

    // ── set_trust_weight / get_trust_weight ───────────────────────────────

    #[test]
    fn trust_weight_set_and_get_round_trip() {
        let mut store = CardRecordStore::new(100);
        let (record, _) = make_signed_record(1);
        let key = record.key.clone();

        store.set_trust_weight(&key, 0.85);
        assert_eq!(store.get_trust_weight(&key), Some(0.85));
    }

    #[test]
    fn trust_weight_missing_key_returns_none() {
        let store = CardRecordStore::new(100);
        let phantom_key = RecordKey::new(&[0xABu8; 32]);
        assert_eq!(store.get_trust_weight(&phantom_key), None);
    }

    #[test]
    fn trust_weight_overwrite_updates_value() {
        let mut store = CardRecordStore::new(100);
        let (record, _) = make_signed_record(1);
        let key = record.key.clone();

        store.set_trust_weight(&key, 0.5);
        store.set_trust_weight(&key, 0.99);
        assert_eq!(store.get_trust_weight(&key), Some(0.99));
    }

    #[test]
    fn trust_weight_zero_stored_and_retrieved() {
        let mut store = CardRecordStore::new(100);
        let (record, _) = make_signed_record(1);
        let key = record.key.clone();

        store.set_trust_weight(&key, 0.0);
        assert_eq!(store.get_trust_weight(&key), Some(0.0));
    }

    #[test]
    fn trust_weight_independent_per_key() {
        let mut store = CardRecordStore::new(100);
        let (rec_a, _) = make_signed_record(1);
        let (rec_b, _) = make_signed_record(1);
        let key_a = rec_a.key.clone();
        let key_b = rec_b.key.clone();

        store.set_trust_weight(&key_a, 0.3);
        store.set_trust_weight(&key_b, 0.7);

        assert_eq!(store.get_trust_weight(&key_a), Some(0.3));
        assert_eq!(store.get_trust_weight(&key_b), Some(0.7));
    }

    #[test]
    fn revocation_via_higher_seq() {
        let mut store = CardRecordStore::new(100);
        let keypair = generate_keypair();

        let active_card = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 1,
            status: CardStatus::Active,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: blake3::hash(b"manifest").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let signed_active = sign_card(active_card, &keypair.signing_key).unwrap();
        let address = compute_address(&signed_active.record.pubkey);
        let key = RecordKey::new(&address);

        store.put(Record {
            key: key.clone(),
            value: canonical_encode(&signed_active).unwrap(),
            publisher: None,
            expires: None,
        }).unwrap();

        let revoked_card = CardRecord {
            pubkey: keypair.verifying_key.as_bytes().to_vec(),
            seq: 2,
            status: CardStatus::Revoked,
            endpoint: "https://example.com/agent".into(),
            manifest_hash: blake3::hash(b"manifest").as_bytes().to_vec(),
            domain: "example.com".into(),
            label: None,
        };
        let signed_revoked = sign_card(revoked_card, &keypair.signing_key).unwrap();

        store.put(Record {
            key: key.clone(),
            value: canonical_encode(&signed_revoked).unwrap(),
            publisher: None,
            expires: None,
        }).unwrap();

        let stored = store.get(&key).unwrap();
        let final_card: SignedCard = canonical_decode(&stored.value).unwrap();
        assert_eq!(final_card.record.status, CardStatus::Revoked);
        assert_eq!(final_card.record.seq, 2);
    }
}
