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

From the repository root, `corepack yarn rust:build-wasm` builds the combined
host. Use `corepack yarn rust:build-wasm --venue hyperliquid` (or `grvt` / `bitget` / `backpack` / `blofin` / `mexc` / `bingx` / `binance` / `bybit` / `cryptocom` / `okx` / `weex`) to
build a single-venue browser/Workers bundle. Each build replaces the bindings
under `hosts/wasm/pkg/`; its constructor accepts only the compiled venues.
The default Cargo features retain all registered venues. CI replays each single-venue
bundle and applies the existing 1.2 MB budget to each one separately.
## Replay tapes

From the repository root, `corepack yarn rust:test-harness` checks the tape
transport. `rust:private-tape-mexc` generates a synthetic MEXC futures account
tape; `rust:gate --venue mexc --tape <path> --books ''` compares its private
state and command results with TypeScript.

Socket routing memoizes successful default URL identities per replay, for both
engines. Caller IDs and custom URL/query rules bypass this cache; socket state,
subscriptions and reconnect generations are always checked live. HTTP request
identity, validation and the gate's measurement windows remain independent.

An optional `meta` record sets `private` and an epoch in milliseconds. Socket
records identify their connection with a zero-based `socket` ordinal per URL,
including reconnects. This distinguishes MEXC's public and private sockets on
the same URL. HTTP records may include `requestBody`: object keys are compared
canonically, preserving large IDs and ignoring generated `externalOid` only.
Every mutation consumes a recorded response; only reads may reuse their last
snapshot. Public replay advances timers by tape timestamps; private replay is
paced from the epoch after startup. Trimming keeps every HTTP anchor inside the
selected time window so later depth resynchronizations remain reproducible.

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

## Live execution financials

`Cx::fill` retains the original FILLS payload. Venues with authoritative
per-execution values can use `fill_with_financials`: if either optional value is
finite, the payload appends one realized-PnL f64 column followed by one fee f64
column, both indexed by the existing fill rows. Missing values are NaN; zero is
retained. The JavaScript reader exposes optional `realizedPnl` and `fee` arrays.
Legacy rows, IDs and event counts are unchanged, and account snapshots never
consume these events. Synthetic or cumulative-order fills do not invent values.

Financial conventions remain those of each live venue event. Hyperliquid close
events report net realized PnL (`closedPnl - fee`) and the execution fee; opening
fills omit both. MEXC and WEEX retain their live realized-PnL fields and absolute
fees. This does not change the separate gross realized-PnL convention used by
historical execution queries.


Private account-read dispatchers may call `Cx::tag_account_read` after selecting
the signer. The pure `Venue::account_auth_outcome` hook returns only definitive
final evidence, respecting typed validity, existing retry admission and session
generation fences. Core preserves captured slot/scope through streamed and
buffered continuations, suppresses recovery on missing lists, and emits ordered
`Notice::AccountAuth` context to the host. Public/unsigned/trading requests remain
untagged; generic HTTP or business success is not credential evidence.
