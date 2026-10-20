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

From the repository root, `corepack yarn rust:build-wasm` builds separate browser/Workers
and Node packages for every registered venue family. Use
`corepack yarn rust:build-wasm --venue hyperliquid` to build only that family.
Each package lives under `hosts/wasm/pkg/{web,node}/<family>/`; its constructor
accepts only that family's venue names. Browser, Node and Workers loaders select
by venue, and aliases share their family's module. The default Cargo features
retain all registered venues for workspace checks. CI builds isolated modules
and enforces each fixed budget from `harness/wasm-budgets.json`.

The default `@tealstreet/safe-cex-wasm/engine` loader selects the same family
packages as the explicit SDK loaders. Browser entry points share initialization
by family so concurrent accounts and aliases keep one live WASM instance;
failed initialization may retry. Node entry points share Node's module cache.

## Account transport routing

Venues emit real URLs and logical socket roles; the core never chooses a proxy.
`RustEngineExchange` applies the registered safe-cex transport through
`createEngineTransport`, reusing TS routing, proxy authorization and HOW/relay.
Explicit headless/direct options stay direct. `createEngineObject` accepts the
same factory as `createTransport` for account-backed Workers deployments and
refuses proxy/tunnel flags when no factory is configured. The returned
transport's credential-free `engineOptions` carry protocol choices derived from
those same routes. SDK and Workers constructors apply them after caller options;
GRVT therefore uses first-frame session auth on its proxy and cookie upgrade
headers on a direct connection.

Native launchers serialize the TS-resolved, token-free routing policy and supply
a context token callback to `NetworkTransport::with_routing`; see
[`hosts/native/README.md`](hosts/native/README.md). No routing table is compiled
into the native host. Secondary signing keys remain unsupported and fail before
engine startup, so the host cannot silently bypass their distinct read egress.

From the repository root, `corepack yarn rust:build-wasm` builds a separate
browser/Workers and Node package for each venue family. Use
`corepack yarn rust:build-wasm --venue hyperliquid` to build one family.
Bindings live under `hosts/wasm/pkg/{web,node}/<family>/`; aliases such as
`coinbasespot` share their family's package. SDK and Workers loaders select
the family before loading WASM. Cargo's default features retain all venues
for workspace checks; release packages use one `venue-<family>` feature.
CI checks each family against `harness/wasm-budgets.json`.
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
snapshot. MEXC, Bitrue and Orderly public replay advance virtual timers by timestamp groups;
other venues preserve recorded message bursts. Private replay is paced after
startup, with the signing epoch continuous through startup. Trimming keeps every HTTP anchor inside the
selected time window so later depth resynchronizations remain reproducible.

WebSocket batches preserve socket IDs and individual arrival times. JS partitions
at roughly 64 KiB (oversized messages alone), retains at most 16 KiB scratch, and
drains before commands, HTTP, timers, closes and snapshots. Native drains ready
bursts without copying the message text or binary payload. Optional `rust:profile` diagnostics are
separate from the uninstrumented public gate.

## Phemex perpetuals

`phemex` supports PerpetualV2 USDT/USDC contracts through the real-value g-API.
Legacy contract families stay filtered as in TypeScript. `Triggered` displays
as Closed but remains nonterminal; a later native New update can reopen it.
Public books request full depth and replace periodic snapshots.

Fills and order history use bounded symbol-scoped reads (up to 200 rows), public
trades up to 1000; they do not claim exhaustive range history. Generic income
returns an explicit unsupported error pending verified ledger scope and amount
semantics. Rust remains opt-in; live private-account validation and subsequent
TypeScript retirement are separate adoption work.

`rust:private-tape:phemex` generates the account/trading tape. Its explicit
`bootstrap: true` auth/snapshot records are delivered before startup readiness.
Older tapes classify public/private sockets sharing a URL from authentication;
explicit connection ordinals take precedence. Request matching preserves caller
IDs and JSON wire types; only generated IDs and documented volatile fields vary.
