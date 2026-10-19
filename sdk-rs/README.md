# safe-cex Rust engine

One exchange engine for the browser worker and Cloudflare Workers
(WebAssembly) and for servers (native). The design, the gates and the banned
patterns are in [`docs/plans/RUST_REWRITE.md`](../docs/plans/RUST_REWRITE.md).

## Crates

| Crate | Purpose |
| --- | --- |
| `safe-cex-crypto` | Signing primitives: HMAC, Ed25519, P-256, EIP-712, Starknet SNIP-12, Lighter Poseidon2/Schnorr, zkLink |

## Commands

From `sdk-rs/`, with the toolchain pinned by `rust-toolchain.toml`:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets
cargo test --locked --workspace
cargo build --locked --workspace --target wasm32-unknown-unknown
./scripts/check-bans.sh
```

CI runs all of these (`.github/workflows/rust.yml`).

## Replay tapes

From the repository root, `corepack yarn rust:test-harness` checks the tape
transport. `rust:private-tape-mexc` generates a synthetic MEXC futures account
tape; `rust:gate --venue mexc --tape <path> --books ''` compares its private
state and command results with TypeScript.

An optional `meta` record sets `private` and an epoch in milliseconds. Socket
records identify their connection with a zero-based `socket` ordinal per URL,
including reconnects. This distinguishes MEXC's public and private sockets on
the same URL. HTTP records may include `requestBody`: object keys are compared
canonically, preserving large IDs and ignoring generated `externalOid` only.
Every mutation consumes a recorded response; only reads may reuse their last
snapshot. MEXC public replay advances virtual timers by timestamp groups;
other venues preserve recorded message bursts. Private replay is paced after
startup. Trimming keeps every HTTP anchor inside the
selected time window so later depth resynchronizations remain reproducible.


From the repository root, `corepack yarn rust:build-wasm` emits one browser and
Node package per `venue-*` Cargo feature under `hosts/wasm/pkg/{web,node}/<family>`.
The browser and Node loaders take a venue name; aliases share a family module.
CI checks each family against `harness/wasm-budgets.json`.

WebSocket batches preserve socket IDs and individual arrival times. JS partitions
at roughly 64 KiB (oversized messages alone), retains at most 16 KiB scratch, and
drains before commands, HTTP, timers, closes and snapshots. Native drains ready
bursts without copying the message text. Optional `rust:profile` diagnostics are
separate from the uninstrumented public gate.
