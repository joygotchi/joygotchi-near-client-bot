#!/usr/bin/env bash
# Builds every JOYGOTCHI contract to res/*.wasm
#
# Goes through `cargo near build`, not plain `cargo build`. That is not a
# preference: cargo-near runs a `wasm-opt -O` post-step that its own help calls
# "required for correctness of build on rustc >= 1.82", and without it every
# panic on-chain degrades to a bare `unreachable` trap with the message lost —
# `require!`, `.expect()` and even `env::panic_str` alike. See issue #3.
#
# Needs cargo-near >= 0.22; older versions ship a wasm-opt that rejects the
# bulk-memory instructions rustc now emits from std.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p res

need="0.22.0"
have="$(cargo near --version 2>/dev/null | awk '{print $2}')"
if [ -z "$have" ]; then
  echo "cargo-near is not installed: cargo install cargo-near --locked" >&2
  exit 1
fi
if [ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" != "$need" ]; then
  echo "cargo-near $have is too old (need >= $need):" >&2
  echo "  cargo install cargo-near --locked --force" >&2
  exit 1
fi

for crate in game pet-stake mint-router; do
  out="${crate//-/_}"
  ( cd "$crate" && cargo near build non-reproducible-wasm --no-abi >/dev/null 2>&1 )
  cp "target/near/${out}/${out}.wasm" "res/${out}.wasm"
  printf '%-16s %s bytes\n' "${out}.wasm" "$(wc -c < "res/${out}.wasm" | tr -d ' ')"
done
