use anyhow::{Context, Result};
use libp2p::identity::Keypair;
use std::fs;
use std::path::Path;

const KEYPAIR_FILE: &str = "node.key";

pub fn load_or_generate(data_dir: &Path) -> Result<Keypair> {
    fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create data dir: {}", data_dir.display()))?;

    let key_path = data_dir.join(KEYPAIR_FILE);

    if key_path.exists() {
        let bytes = fs::read(&key_path)
            .with_context(|| format!("failed to read keypair from {}", key_path.display()))?;
        let keypair = Keypair::from_protobuf_encoding(&bytes)
            .context("failed to decode keypair (corrupt file?)")?;
        tracing::info!(peer_id = %keypair.public().to_peer_id(), "Loaded existing keypair");
        Ok(keypair)
    } else {
        let keypair = Keypair::generate_ed25519();
        let bytes = keypair.to_protobuf_encoding()
            .context("failed to encode keypair")?;
        fs::write(&key_path, &bytes)
            .with_context(|| format!("failed to write keypair to {}", key_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
        }
        tracing::info!(peer_id = %keypair.public().to_peer_id(), path = %key_path.display(), "Generated new keypair");
        Ok(keypair)
    }
}
