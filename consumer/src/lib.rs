mod error;
mod resolve;

pub use error::ConsumerError;
pub use resolve::{verify_fetched_card, verify_manifest, fetch_and_verify_manifest};
