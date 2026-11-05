#!/usr/bin/env bash
# Upgrade a live pet-stake deployment in place, keeping its state.
#
#   ./migrate-testnet.sh <suffix> <oracle-account>
#
# `deploy-testnet.sh` runs `new`, which cannot run twice and would leave the
# contract with empty state anyway: every staked pet, credit balance, inventory
# and listing gone. This runs `migrate` instead — an `#[init(ignore_state)]`
# entry point that reads the previous Borsh layout and adds the `oracle` field.
#
# Why a migration rather than fresh accounts: on testnet the difference is a
# faucet call, but the same change on mainnet has no fresh-accounts option at
# all, and this repository has already lost two deployments to a struct change
# (docs/GAME_CONTRACT_V2.md §9). Doing it the migration way here is the only
# way to find out whether the migration works.
#
# Deploy and migrate go in one transaction, so there is no window in which the
# new code is live over state it cannot read.
set -euo pipefail

SUF="${1:?usage: ./migrate-testnet.sh <suffix> <oracle-account>}"
ORACLE="${2:?usage: ./migrate-testnet.sh <suffix> <oracle-account>}"
cd "$(dirname "$0")"

STAKE="jg-stake-$SUF.testnet"

echo "── $STAKE  (oracle: $ORACLE)"
near contract deploy "$STAKE" use-file res/pet_stake.wasm \
  with-init-call migrate json-args "{\"oracle\":\"$ORACLE\"}" \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send

echo
echo "── state after the migration (every number should be unchanged)"
near contract call-function as-read-only "$STAKE" get_pool json-args '{}' \
  network-config testnet now
near contract call-function as-read-only "$STAKE" get_config json-args '{}' \
  network-config testnet now
