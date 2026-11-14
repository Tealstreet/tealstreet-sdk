# safe-cex Rust engine

One exchange engine for the browser worker and Cloudflare Workers
(WebAssembly) and for servers (native). The design, the gates and the banned
patterns are in [`docs/plans/RUST_REWRITE.md`](../docs/plans/RUST_REWRITE.md).

## Crates

Local worker TWAP restore ownership is optional for compatibility with existing
JS hosts. `EngineHost.algoStart(accountName, twapOwner?)` accepts a trusted
`{ ownerId: string, generation: number }` snapshot (nonempty owner and a safe,
nonnegative integer generation). It calls the additive WASM entry point
`algoStartOwned(accountName, twapOwnerJson, now, wall)`; the JSON contains exactly
`ownerId` and `generation`. Existing three-argument WASM `algoStart` remains
unchanged and retains the prior account-name-only restore behavior when the
owner parameter is absent. This compatibility exception is interim; explicit
null/invalid ownership never selects it. Bound restores require both original
persisted tags; untagged saved rows do not resume.

The native server initially binds TWAPs to the verified registry identity digest when present,
otherwise the authenticated user, with a generation derived from that binding.
Same-owner reconnects, resumes and signing handovers keep the current generation,
so the approved 60-second COSIGNED resume
window keeps running TWAPs. Fresh bindings for the same authenticated identity
use the same initial generation across account rebuilds, restarts and box moves.
An in-place authenticated owner change advances the binding's generation and
drops the old owner's pending slices, even when returning to an earlier owner;
a retired generation is never reinstated on that binding.
Bound restores refuse pre-release-8 untagged rows with a visible error; see the
[TWAP ownership contract](algos/twap/README.md) for the exact messages.

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

## WebSocket control and mutation frames

`Cx::send` and `Cx::send_signed` consume `ControlFrame`. Venue encoding helpers
declare subscriptions (including unsubscriptions and subscription snapshots),
session authentication or heartbeats with `ControlFrame::subscription`,
`ControlFrame::authentication` or `ControlFrame::heartbeat`. These declarations
are protocol contracts: neither the engine nor the host infers authority from
JSON method strings. Signed control recipes must preserve the declared purpose.
Raw text and `WebSocketWrite` cannot enter either control-send API.

Order and setting mutations use `Cx::websocket_write` with a live command
Mutation Context, followed by `Cx::send_websocket_write`. The host must enable
the typed write handoff and consume its single-use binding through
`Engine::claim_websocket_write` immediately before the physical send. The shared
write lifecycle reports transport-neutral timeout and unproven-outcome errors.

## Browser request-failure diagnostics

The JS host emits `http.request_failed` at Debug for failed managed HTTP work,
including engine deadline refusals during preparation or after release. The native
notice carries only its exact opaque request ID; the JS host joins the original
resource before cancellation, without inferring a timeout from elapsed time.
Facts contain only a closed HTTP method, phase, failure kind and a path template:
fixed API vocabulary survives, all other segments become `:segment`, at most eight
segments plus `:more`. Origins, queries, fragments, bodies and headers are excluded.
The SDK adds the actual venue; consumers may retain a bounded recent breadcrumb.
These diagnostics neither retry requests nor add a second user-facing error.

## Startup discovery deadlines

Before both markets and tickers load, credential-free Background Reads using the
standard 20 s policy receive a 90 s total deadline: 60 s for discovery plus a
30 s transport-readiness allowance. The host caps readiness within that original
deadline; it never resets the clock after readiness. Credentialed reads, caller
commands, writes, explicit venue timeouts and reads after discovery keep their
existing bounds.

## HTTP request classes

The TS HOW package's `requestBudget.ts` defines the parity reference: catalogue
reads have 60 s, history pages 30 s, and order reads, writes and other reads 20 s.
Engine history queries (fills, order history and income) use 30 s. Candles,
public trades, live account/order reads and mutations use 20 s. Background
catalogue/metadata reads alone use discovery: markets, instruments, contracts,
symbol/asset/product catalogues and closed catalogue RPC kinds. Discovery keeps
60 s, or the existing 90 s startup discovery/readiness allowance.

Background issuance reserves a bounded catalogue ceiling while the semantic
request is not yet available. The shared managed-HTTP boundary resolves the
request and caps its physical deadline from the original Context issuance time.
Depth/order books, tickers/market summaries, trades, candle polls, funding/mark
prices and unknown public read kinds use 20 s on every family. Catalogue matching
uses closed path components/RPC discriminators, never query parameter values.
Explicit nonstandard venue bounds and shorter Plan/caller caps still win.
Re-preparation and retry cannot restart the original class deadline.

Native hosts query `http_request_class` only for an exact pending preparation.
Browser preparation actions carry a compact `u8` class outside signed packet
bytes (discovery 0, orderRead 1, orderWrite 2, history 3, read 4). The JS host
maps that code to a closed label and formats timeout diagnostics. Native-only
class names, legacy path classification and diagnostic formatting are excluded
from wasm; engine issuance retains numeric bounded deadlines. TS conservatively
labels non-GET methods `orderWrite` regardless of hints while honoring the
explicit remaining engine deadline. Labels grant no send or retry authority.
Timeouts name class, budget and elapsed milliseconds, including host diagnostics
when engine expiry wins the timer race. HOW readiness remains capped at 30 s
inside the same original deadline. Legacy GET paths use exact catalogue/history/
order components; unknown paths use the 20 s read default and non-GET requests
conservatively use the short write default. Explicit transport deadlines win.
No server, retry, outcome-unknown or socket-silence policy changes are needed.

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
Before startup, replacing or removing the signing provider retires its queued
signing and unclaimed signed packets with `notSent`; old completions cannot
release them. Reinstalling the same provider handle preserves pending work.

Rust correctness and performance replays each install an isolated identity
stream from the exact tape SHA-256. The landed Bitget SHA/domain/counter byte
stream and full raw-ID comparisons are retained. Production crypto globals are
never patched, and signing stays cryptographic unless a test explicitly supplies
its separate source. Test fixtures deliberately inject both sources through the
`test-support` feature; production constructors have no implicit seeded mode.

## Account transport routing

Hosted local setup captures `TrustedSignerState` after the registry consumes
signing material. Required public wallet metadata remains in account options;
opaque non-JSON setup facts travel with native `ServerConfig` and refuse a
different venue. Client JSON cannot assert trusted derived-key facts. The same
state accompanies both the admission probe and resident construction.

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
Live candles accept only `kline_p` incremental frames; subscription snapshots
and backfill never publish live candles. Candle timestamps remain in seconds.
Each public frame parse failure emits a `stream.malformed` warning with
the raw frame's byte count and wire `type` (`unknown` for unreadable JSON).

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
