#!/usr/bin/env bash
# Deploy all three contracts to testnet and wire them together.
#
#   ./deploy-testnet.sh <suffix> [oracle-account]
#
# Expects three accounts to exist already, each with ~10 NEAR:
#   jg-game-<suffix>.testnet  jg-stake-<suffix>.testnet  jg-mint-<suffix>.testnet
#
# `oracle-account` is the account whose key the AI backend signs
# `award_feed_xp` with, so a feed raises the pet's XP on chain. Left out, the
# oracle starts as the staking contract's owner and `set_oracle` names it
# later. See "The feed-XP oracle" in README.md for how to make its key.
#
# This re-initialises state. To upgrade a deployment that is already live, use
# ./migrate-testnet.sh instead — `new` cannot run twice, and a redeploy with
# fresh state costs every player their staked pet and their credit balance.
#
# Create them with:
#   near account create-account sponsor-by-faucet-service jg-game-<suffix>.testnet \
#     autogenerate-new-keypair save-to-keychain network-config testnet create
set -euo pipefail

SUF="${1:?usage: ./deploy-testnet.sh <suffix> [oracle-account]}"
ORACLE="${2:-}"
cd "$(dirname "$0")"

GAME="jg-game-$SUF.testnet"
STAKE="jg-stake-$SUF.testnet"
MINT="jg-mint-$SUF.testnet"
WNEAR="wrap.testnet"

# 2 NEAR per pet, 0.5 NEAR per AI credit, 500 credits per 30-day epoch.
MINT_PRICE="2000000000000000000000000"
NEAR_PER_CREDIT="500000000000000000000000"
EPOCH_BUDGET="500000000"

deploy() {
  local account="$1" wasm="$2" args="$3"
  echo "── $account"
  near contract deploy "$account" use-file "$wasm" \
    with-init-call new json-args "$args" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    network-config testnet sign-with-keychain send
}

call() {
  local signer="$1" receiver="$2" method="$3" args="$4" deposit="${5:-0 NEAR}"
  near contract call-function as-transaction "$receiver" "$method" \
    json-args "$args" prepaid-gas '100.0 Tgas' attached-deposit "$deposit" \
    sign-as "$signer" network-config testnet sign-with-keychain send
}

# The account names are known up front, so the three init calls can reference
# each other without a bootstrap step.
deploy "$GAME"  res/game.wasm \
  "{\"owner_id\":\"$GAME\",\"stake_contract\":\"$STAKE\",\"mint_router\":\"$MINT\",\"treasury\":\"$MINT\",\"mint_price\":\"$MINT_PRICE\"}"

deploy "$STAKE" res/pet_stake.wasm \
  "{\"owner_id\":\"$STAKE\",\"game_contract\":\"$GAME\",\"granter\":\"$MINT\",\"epoch_budget\":\"$EPOCH_BUDGET\",\"oracle\":$([ -n "$ORACLE" ] && echo "\"$ORACLE\"" || echo null)}"

deploy "$MINT"  res/mint_router.wasm \
  "{\"owner_id\":\"$MINT\",\"game_contract\":\"$GAME\",\"stake_contract\":\"$STAKE\",\"wnear\":\"$WNEAR\",\"treasury\":\"$MINT\",\"mint_price\":\"$MINT_PRICE\",\"near_per_credit\":\"$NEAR_PER_CREDIT\"}"

echo
echo "── wiring"
# The router has to be able to hold wNEAR before 1Click can settle into it.
call "$MINT" "$WNEAR" storage_deposit "{\"account_id\":\"$MINT\"}" '0.00125 NEAR'

# Credits the treasury has "bought", backing sales, mining and the faucet.
call "$STAKE" "$STAKE" fund_reserve '{"credits":"50000000"}'

echo
echo "GAME  = $GAME"
echo "STAKE = $STAKE"
echo "MINT  = $MINT"
[ -n "$ORACLE" ] && echo "ORACLE = $ORACLE" || true
