#!/bin/bash
# Generate a properly signed card JSON using p2s-card-gen binary
# Usage: gen_card.sh <domain> <seq> [status]
# Output: JSON suitable for POST /cards

DOMAIN=$1
SEQ=${2:-1}
STATUS=${3:-active}

# Use a deterministic seed based on domain for reproducible keys
SEED=$(echo -n "$DOMAIN" | sha256sum | cut -c1-64)

# Call the Rust helper
/home/vovkes/p2s/target/release/p2s-card-gen "$DOMAIN" "$SEQ" "$STATUS" 2>/dev/null
