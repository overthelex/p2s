use p2s_card::{generate_keypair, sign_card, compute_address, CardRecord, CardStatus};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let domain = args.get(1).map(|s| s.as_str()).unwrap_or("test.p2s");
    let seq: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let status = match args.get(3).map(|s| s.as_str()) {
        Some("revoked") => CardStatus::Revoked,
        _ => CardStatus::Active,
    };

    let keypair = generate_keypair();
    let endpoint = format!("https://{domain}/mcp");
    let manifest_hash = blake3::hash(format!("manifest-{domain}").as_bytes()).as_bytes().to_vec();

    let record = CardRecord {
        pubkey: keypair.verifying_key.as_bytes().to_vec(),
        seq,
        status,
        endpoint,
        manifest_hash,
        domain: domain.to_string(),
        label: Some(domain.to_string()),
    };

    let signed = sign_card(record, &keypair.signing_key).unwrap();
    let address = compute_address(&signed.record.pubkey);

    let json = serde_json::json!({
        "record": {
            "pubkey": hex::encode(&signed.record.pubkey),
            "seq": signed.record.seq,
            "status": match signed.record.status {
                CardStatus::Active => "active",
                CardStatus::Revoked => "revoked",
            },
            "endpoint": signed.record.endpoint,
            "manifest_hash": hex::encode(&signed.record.manifest_hash),
            "domain": signed.record.domain,
            "label": signed.record.label,
        },
        "sig": hex::encode(&signed.sig),
        "_address": hex::encode(address),
    });

    println!("{}", serde_json::to_string(&json).unwrap());
}
