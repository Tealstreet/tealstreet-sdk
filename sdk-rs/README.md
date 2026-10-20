# safe-cex Rust engine

One exchange engine for the browser worker and Cloudflare Workers
(WebAssembly) and for servers (native). The design, the gates and the banned
patterns are in [`docs/plans/RUST_REWRITE.md`](../docs/plans/RUST_REWRITE.md).

## Crates

| Crate | Purpose |
| --- | --- |
| `safe-cex-crypto` | Signing primitives: HMAC, Ed25519, P-256, EIP-712, Starknet SNIP-12, Lighter Poseidon2/Schnorr, zkLink |

EIP-712 venues encode their fixed domain and message words locally. Shared crypto
combines their hashes with the EIP-191 `0x1901` prefix and signs the digest; the
dynamic ABI parser is used only by independent tests.

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

## Account transport routing

Venues emit real URLs and logical socket roles; the core never chooses a proxy.
`RustEngineExchange` applies the registered safe-cex transport through
`createEngineTransport`, reusing TS routing, proxy authorization and HOW/relay.
Explicit headless/direct options stay direct. `createEngineObject` accepts the
same factory as `createTransport` for account-backed Workers deployments and
refuses proxy/tunnel flags when no factory is configured.

Native launchers serialize the TS-resolved, token-free routing policy and supply
a context token callback to `NetworkTransport::with_routing`; see
[`hosts/native/README.md`](hosts/native/README.md). No routing table is compiled
into the native host. Secondary signing keys remain unsupported and fail before
engine startup, so the host cannot silently bypass their distinct read egress.

From the repository root, `corepack yarn rust:build-wasm` builds the combined
host. Use `corepack yarn rust:build-wasm --venue hyperliquid` (or `bitget` / `backpack` / `blofin` / `mexc` / `bingx` / `binance` / `bybit` / `cryptocom` / `okx` / `weex`) to
build a single-venue browser/Workers bundle. Each build replaces the bindings
under `hosts/wasm/pkg/`; its constructor accepts only the compiled venues.
The default Cargo features retain all registered venues. CI replays each single-venue
bundle and applies the existing 1.2 MB budget to each one separately.
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
snapshot. Public replay advances timers by tape timestamps; private replay is
paced from the epoch after startup. Trimming keeps every HTTP anchor inside the
selected time window so later depth resynchronizations remain reproducible.
