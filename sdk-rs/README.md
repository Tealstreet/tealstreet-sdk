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
