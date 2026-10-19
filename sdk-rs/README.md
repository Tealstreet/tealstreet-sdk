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

From the repository root, `corepack yarn rust:build-wasm` builds the combined
host. Use `corepack yarn rust:build-wasm --venue hyperliquid` (or `binance`) to
build a single-venue browser/Workers bundle. Each build replaces the bindings
under `hosts/wasm/pkg/`; its constructor accepts only the compiled venues.
The default Cargo features retain both venues. CI replays each single-venue
bundle and applies the existing 1.2 MB budget to each one separately.
