use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "p2s-node", about = "P2S Distributed Agent-Card Registry Node")]
pub struct Cli {
    /// libp2p listen multiaddr
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/4001")]
    pub listen: String,

    /// HTTP API port
    #[arg(long, default_value_t = 8080)]
    pub http_port: u16,

    /// Bootstrap peer multiaddr (repeatable), format: /ip4/x.x.x.x/tcp/port/p2p/<PeerId>
    #[arg(long)]
    pub bootstrap_peer: Vec<String>,

    /// Data directory for keypair and state
    #[arg(long, default_value = "./data")]
    pub data_dir: PathBuf,

    /// Trusted validator public key in hex (repeatable, 32 bytes = 64 hex chars)
    #[arg(long)]
    pub validator_key: Vec<String>,
}
