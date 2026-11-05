#!/usr/bin/env bash
# Builds every JOYGOTCHI contract to res/*.wasm
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p res

cargo build --release --target wasm32-unknown-unknown

for crate in joy_token pet_stake mint_router; do
  src="target/wasm32-unknown-unknown/release/${crate}.wasm"
  cp "$src" "res/${crate}.wasm"
  printf '%-16s %s bytes\n' "${crate}.wasm" "$(wc -c < "res/${crate}.wasm" | tr -d ' ')"
done
