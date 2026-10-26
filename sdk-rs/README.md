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

## Host entropy

Core construction is explicit: `Engine::new(venue, Some(entropy))` returns a
`Result`, and `None` is a typed `InvalidInput` error before startup. Registry
`build` takes the same provider argument. Native production callers use
`safe_cex_native::production_entropy()`; the frame server supplies it automatically.
Replay hosts can replace identity using `Host::with_entropy` before startup.

WASM construction binds WebCrypto without new crypto dependencies. `EngineHost`
uses the same secure defaults in browser workers, Cloudflare Workers, and the
Node/Bun SDK paths used by CLI and exchange-mcp. Its optional fourth argument
accepts `entropy: (length: number) => Uint8Array`; the host copies and validates
those bytes for client IDs, jitter and receipt sessions. `signingEntropy` is a
separate, explicit test override. Seeding identity never changes Lighter signing
nonces, Coinbase JWTs or other request-signing randomness. No provider or seed is
serialized into account options or commands. Both raw WASM setters and native
setters reject replacement after startup. Provider failures stop ID/signature
creation before send; retry jitter alone uses the nominal delay on failure.

Rust correctness and performance replays each install an isolated identity
stream from the exact tape SHA-256. The landed Bitget SHA/domain/counter byte
stream and full raw-ID comparisons are retained. Production crypto globals are
never patched, and signing stays cryptographic unless a test explicitly supplies
its separate source. Test fixtures deliberately inject both sources through the
`test-support` feature; production constructors have no implicit seeded mode.

## Account transport routing

Venues emit real URLs and logical socket roles; the core never chooses a proxy.
`RustEngineExchange` applies the registered safe-cex transport through
`createEngineTransport`, reusing TS routing, proxy authorization and HOW/relay.
Explicit headless/direct options stay direct. `createEngineObject` accepts the
same factory as `createTransport` for account-backed Workers deployments and
refuses proxy/tunnel flags when no factory is configured. Deployments must also
supply `accountToken(state, env)`, resolving a deployment-managed token unique to
that object/account; upgrades require `Authorization: Bearer <token>` and fail
closed when the resolver is absent. Never select an account or token from the
first client’s start message. Clients sharing that token share account access;
start cannot replace a running or pending engine. The returned
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
host under `hosts/wasm/pkg/{web,node}`. Node (servers, the CLI, MCP and tests)
and React Native load it for every venue. `corepack yarn rust:build-wasm
--families` builds one package per venue family under
`hosts/wasm/pkg/{web,node}/<family>`. The default browser/Workers loader
(`@tealstreet/safe-cex-wasm/engine`) fetches only the family of the account's
venue (`js/venues.mjs` maps every venue name to its family), and a bundler
emits one chunk per family. Any browser or Metro bundle that includes safe-cex
resolves all 28 family imports, so run `--families` before bundling. A family
package is about a tenth of the combined engine's bytes and cold-load CPU
(`docs/plans/RUST_WASM_FAMILIES.md`).
`--venue hyperliquid` (or any venue name) builds that venue's family in place
of the combined bindings, for replay gates. CI applies the 1.2 MB budget to
every family package.
## Replay tapes

From the repository root, `corepack yarn rust:test-harness` checks the tape
transport. `rust:private-tape-mexc` generates a synthetic MEXC futures account
tape; `rust:gate --venue mexc --tape <path> --books ''` compares its private
state and command results with TypeScript.

For faster private replay iteration, download a recent matching CI engine:

```bash
gh run download RUN_ID --name ci-wasm-binance --dir /tmp/binance-ci-wasm
corepack yarn -s rust:gate:record --ci-wasm /tmp/binance-ci-wasm \
  --venue binance --tape sdk-rs/harness/tapes/binance-private.jsonl --runs 5 --books ''
```

The `ci-wasm-<suite>` artifacts expire after three days. The gate checks the
fixed Rust/build-input tree against HEAD, requires clean covered inputs and
validates the family, pinned tool versions, build flags and all package hashes.
It refuses missing or mismatched artifacts with no fallback to another wasm
build. Harness-only clock/declaration edits can reuse the same engine. Reports
record its provenance. Local replays help iteration; full green CI on the PR
head is still required before merge. Existing replay reports retain fourteen
days of history, without a second copy of the wasm binaries.

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

FILLS writes revision 2: existing rows and symbol references, optional `Fill.id`
execution references, then aligned realized-PnL and fee f64 columns. Missing IDs
use the wire NONE reference; missing financials are NaN and zero is retained.
Venues use `fill_with_financials` for authoritative per-execution values.
`fillId`, `realizedPnl` and `fee` expose those columns in JavaScript. The reader
also accepts legacy revision 0/1, including revision 1 with or without financial
tails. SDK event `id` uses execution identity when present, while `orderId`
retains attribution. Account snapshots never consume these events. Synthetic or
cumulative-order fills do not invent execution IDs or financial values.

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
