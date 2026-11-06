#!/usr/bin/env bash
# Upgrade a live JOYGOTCHI deployment in place, keeping its state.
#
#   ./migrate-testnet.sh <suffix> [oracle-account]
#
# `deploy-testnet.sh` runs `new`, which cannot run twice and would leave the
# contracts with empty state anyway: every staked pet, credit balance, inventory
# and listing gone. This upgrades the code instead.
#
# Why a migration rather than fresh accounts: on testnet the difference is a
# faucet call, but the same change on mainnet has no fresh-accounts option at
# all, and this repository has already lost two deployments to a struct change
# (docs/GAME_CONTRACT_V2.md §9). Doing it the migration way here is the only
# way to find out whether the migration works.
#
# Two contracts, two different problems:
#
#   game       Only gained a *method* (`feed_from_stake`). A method is not
#              stored state, so the struct layout is untouched and a plain code
#              deploy with no init call keeps every pet exactly as it is.
#
#   pet-stake  Gained a field on the contract struct, which is a Borsh layout
#              change and needs an `#[init(ignore_state)]` entry point that
#              reads the old layout. There is one per layout this contract has
#              ever been deployed with, each going straight to the current one:
#
#                migrate(oracle)      from the pre-`oracle` layout
#                migrate_auto_feed()  from the `oracle` layout (what is live)
#
#              Which one is needed depends on what the account currently holds,
#              so this script asks it rather than making the caller remember.
#              `oracle-account` is only used on the first path, and is required
#              only if that is the path taken.
#
# Deploy and migrate go in one transaction, so there is no window in which the
# new code is live over state it cannot read.
set -euo pipefail

SUF="${1:?usage: ./migrate-testnet.sh <suffix> [oracle-account]}"
ORACLE="${2:-}"
cd "$(dirname "$0")"

GAME="jg-game-$SUF.testnet"
STAKE="jg-stake-$SUF.testnet"

for wasm in res/game.wasm res/pet_stake.wasm; do
  [ -f "$wasm" ] || { echo "$wasm is missing — run ./build.sh first" >&2; exit 1; }
done

echo "── $GAME  (code only: the struct is unchanged, so no init call)"
near contract deploy "$GAME" use-file res/game.wasm \
  without-init-call \
  network-config testnet sign-with-keychain send

echo
echo "── $STAKE  (deciding which migration the live state needs)"
CONFIG="$(near contract call-function as-read-only "$STAKE" get_config json-args '{}' \
  network-config testnet now 2>/dev/null || true)"

if printf '%s' "$CONFIG" | grep -q '"auto_feed_window_seconds"'; then
  echo "   already on the auto-feed layout — deploying code only"
  near contract deploy "$STAKE" use-file res/pet_stake.wasm \
    without-init-call \
    network-config testnet sign-with-keychain send
elif printf '%s' "$CONFIG" | grep -q '"oracle"'; then
  echo "   oracle layout → migrate_auto_feed"
  near contract deploy "$STAKE" use-file res/pet_stake.wasm \
    with-init-call migrate_auto_feed json-args '{}' \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    network-config testnet sign-with-keychain send
else
  : "${ORACLE:?this deployment predates the oracle field, so an oracle account is required: ./migrate-testnet.sh $SUF <oracle-account>}"
  echo "   pre-oracle layout → migrate (oracle: $ORACLE)"
  near contract deploy "$STAKE" use-file res/pet_stake.wasm \
    with-init-call migrate json-args "{\"oracle\":\"$ORACLE\"}" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    network-config testnet sign-with-keychain send
fi

echo
echo "── state after the migration (every number should be unchanged)"
near contract call-function as-read-only "$STAKE" get_pool json-args '{}' \
  network-config testnet now
near contract call-function as-read-only "$STAKE" get_config json-args '{}' \
  network-config testnet now
