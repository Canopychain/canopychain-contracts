#!/usr/bin/env bash
# Builds both contracts, deploys them to Stellar testnet, initializes each
# with the deploying identity as admin, and writes the resulting contract
# IDs to deployments.json at the repo root.
#
# Requires the `stellar` CLI (https://developers.stellar.org/docs/tools/cli)
# and a funded testnet identity. Run from the repo root:
#
#   STELLAR_SOURCE_ACCOUNT=my-testnet-identity ./scripts/deploy-testnet.sh

set -euo pipefail

NETWORK="testnet"
SOURCE_ACCOUNT="${STELLAR_SOURCE_ACCOUNT:?Set STELLAR_SOURCE_ACCOUNT to a funded testnet identity name}"
WASM_DIR="target/wasm32-unknown-unknown/release"
DEPLOYMENTS_FILE="deployments.json"

echo "Building contracts (release, wasm32-unknown-unknown)..."
cargo build --workspace --target wasm32-unknown-unknown --release

deploy_contract() {
  local wasm_name="$1"
  stellar contract deploy \
    --wasm "$WASM_DIR/${wasm_name}.wasm" \
    --source "$SOURCE_ACCOUNT" \
    --network "$NETWORK"
}

echo "Deploying project-registry..."
PROJECT_REGISTRY_ID=$(deploy_contract "project_registry")
echo "  -> $PROJECT_REGISTRY_ID"

echo "Deploying milestone-vault..."
MILESTONE_VAULT_ID=$(deploy_contract "milestone_vault")
echo "  -> $MILESTONE_VAULT_ID"

echo "Initializing project-registry (admin: $SOURCE_ACCOUNT)..."
stellar contract invoke \
  --id "$PROJECT_REGISTRY_ID" \
  --source "$SOURCE_ACCOUNT" \
  --network "$NETWORK" \
  -- init --admin "$SOURCE_ACCOUNT"

echo "Initializing milestone-vault (admin: $SOURCE_ACCOUNT)..."
stellar contract invoke \
  --id "$MILESTONE_VAULT_ID" \
  --source "$SOURCE_ACCOUNT" \
  --network "$NETWORK" \
  -- init --admin "$SOURCE_ACCOUNT"

cat > "$DEPLOYMENTS_FILE" <<EOF
{
  "network": "$NETWORK",
  "deployed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "admin": "$SOURCE_ACCOUNT",
  "contracts": {
    "project-registry": "$PROJECT_REGISTRY_ID",
    "milestone-vault": "$MILESTONE_VAULT_ID"
  }
}
EOF

echo "Wrote $DEPLOYMENTS_FILE"
