# Coinbase Rust venue port

Branch: `feat/rust-coinbase`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and acceptance: `VENUE_PORT_BRIEF.md`.

## Scope and boundaries

One `safe-cex-venue-coinbase` crate serves `coinbase` and `coinbasespot`.
Preserve active Advanced Trade INTX perpetuals, CFM dated futures plus spot,
and the saved-account spot-only variant. Portfolio permission discovery decides
the authenticated product family. Spot-only must never probe CFM or INTX private
routes. Public-only INTX discovery remains keyless; the spot-only profile uses the documented unauthenticated Advanced Trade market routes. Starbase modules are inert
in TS and remain outside runtime activation; do not guess their unresolved units
or private notification envelopes. Typed records replace the deleted first-port
JSON emulation described by stale Coinbase documentation.

## Phases

1. Public data: typed streamed discovery and ticker polling, full-depth sequenced
   books, candle stream/fetch, symbol aliases, subscriptions and socket watchdogs.
   Verify documented examples, gaps, reconnects, malformed snapshots and aliases;
   independent code-correctness review, fix, commit.
2. Private data and native JWT: portfolio selection, balances, positions, settings,
   paginated open orders, user snapshot batches and incremental executions.
   Stale/failed reads preserve newer state. Auth plan below is audited against
   official sources before implementation. Verify both algorithms, REST/user
   auth, permission probes, snapshot races, fill replay and CFM units; review,
   fix, commit.
3. Trading: preflight full batches and caller IDs; Market, Limit and StopLoss
   configurations; CFM quantity conversion; stable-ID spot edits with exact
   provenance; confirmed INTX cancel/replace; per-ID batch cancellation; fixed
   one-way/cross/leverage behavior. No automatic write retries. Verify ambiguous
   responses and cancellation failures; review, fix, commit.
4. Queries: bounded documented history/public-trade reads with typed output,
   pagination and explicit unsupported axes/filters. Never manufacture reported
   PnL by folding fills. Verify limits/cursors/errors; review, fix, commit.
5. Harness and gates: SDK/WASM/native registration for both aliases; freeze the
   TS reference outside production and remove replaced runtime code; documented
   private tapes with independent JWT verification; live two-book public fixture,
   declared TS corrections, identical-output/performance gates; Binance controls;
   bans/fmt/Clippy/native/WASM/host/SDK checks. Review, fix, commit.

After all gates pass, merge latest landing branch, rerun affected gates, push,
open PR into `feat/rust-rewrite`, post independent correctness reviews and fixes,
repeat until clear or diminishing returns. Merge only on exact-head green CI.
Cut a fresh follow-up branch from updated landing. Never stash/rebase/force-push
or touch master. Record measurements and unresolved evidence honestly.

## JWT design and source audit

The signer is owned by the venue, imported once, with no Debug or private export.
Credential input is bounded to 16 KiB per field before parsing. Typed downloaded
JSON accepts `name` and `privateKey`, `private_key`, `keySecret`, `key_secret`,
`secret`; secret-field JSON takes precedence over key-field JSON, as in TS.
Qualified names are exactly `organizations/{org}/apiKeys/{key}`, with nonempty
segments and no whitespace/control characters. Errors never echo input.
Decoded buffers, normalized private strings, parsed JSON secret fields, and
Ed25519 seeds zeroize when dropped.

P-256 SEC1 PEM is decoded and wrapped in a bounded PKCS8 PrivateKeyInfo with
`ecPublicKey`/P-256 OIDs; PKCS8 PEM imports directly. Escaped newlines normalize
before decoding. `safe-cex-crypto::es256::Es256SigningKey` validates the curve and
signs SHA-256 over exact JWS input using RFC 6979. The signature is fixed-width
64-byte `r || s`, never DER. Deterministic Rust ECDSA signatures can differ from
randomized browser ECDSA while both verify; harnesses must verify signed bytes
rather than compare signatures.

Ed25519 accepts standard or URL-safe base64, padded or unpadded, whitespace and
an optional case-insensitive `ed25519:` prefix. Decoded length must be 32 or 64;
64-byte CDP keypairs use their first 32-byte seed, matching TS and the official
Advanced Trade Python SDK. The appended public half is not treated as a second
source of authority or a signing input. Signing uses
`safe-cex-crypto::ed25519::sign` on exact input bytes without prehash.

Typed JOSE headers contain algorithm, `typ: JWT`, exact `kid`, and a fresh nonce
from `safe-cex-crypto::uuid::uuid`, which already uses native `getrandom` or the
browser cryptographic random source. Its TS-compatible alphanumeric nonce need
not match the documentation's example hexadecimal spelling. Empty entropy
output fails closed; no time/counter fallback exists. Private tests inject a
fixed nonce only inside the signer module. Three JWS parts use base64url without
padding. Engine milliseconds are floored to seconds for `nbf`, and `exp` is
exactly 120 seconds later; non-finite, negative or overflowing clocks reject.

REST signs an uppercase method, fixed real venue host `api.coinbase.com`, and
the resolved endpoint pathname, excluding query, fragment, scheme, routing
prefix and body. Relative absolute-path inputs are validated, and double-slash
network paths reject. Sign when dispatching after any pacing, never when a job
is queued. Native requests use the venue's standard `Authorization: Bearer`
header and JSON content type; generic hosts preserve venue credentials while
applying routing. The former TS proxy header alias and copied Binance API-key
header are not venue authentication requirements.

Each private `user` and `heartbeats` subscribe gets a different fresh JWT with
no URI. Subscribe immediately after open, with a fresh token on reconnect;
discard stale socket callbacks and incomplete user snapshot staging. Watchdog
health follows real server heartbeats. JWT expiry does not justify inventing an
application ping, replaying old subscriptions or unsolicited periodic renewals.

Claim compatibility is explicit: preserve active TS ES256 `iss: coinbase-cloud`
without `aud`, and EdDSA `iss: cdp` with `aud: [cdp_service]`. Current official
sources conflict: App documentation warns ECDSA-only, while its newest official
Advanced Trade SDK loads Ed25519 as well as P-256 and uses `iss: cdp` without
`aud` for both. General CDP JWT samples use `cdp`/`cdp_service`; a WebSocket C#
sample still uses `coinbase-cloud`. This port documents the contradiction;
without an account capture it does not label the existing claim spelling a
proven bug or silently switch it.

Independent tests verify both P-256 PEM formats using the p256 verifier and
Ed25519 using the RFC 8032 seed/public-key vector; inspect claims, REST URI,
64-byte signatures, fresh per-message nonces, absent WS URI, credential JSON
precedence, malformed secrets, and clock/path boundaries. Mutation and wrong-key
verification reject. Production signing stays entirely in Rust.

Primary references checked 2026-10-20:

- [Coinbase App API-key authentication](https://docs.cdp.coinbase.com/coinbase-app/authentication-authorization/api-key-authentication):
  qualified names, PEM/newlines, standard Bearer header, 120-second lifetime.
- [Advanced Trade WebSocket authentication](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-authentication):
  WS tokens omit REST method/path; source examples retain conflicting issuers.
- [WebSocket overview](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-overview):
  subscribe within five seconds, one channel/message, new JWT per message.
- [Official Advanced Trade JWT generator](https://github.com/coinbase/coinbase-advanced-py/blob/master/coinbase/jwt_generator.py):
  P-256/Ed25519 key import and raw 32/64-byte Ed25519 handling.
- [Official REST implementation](https://github.com/coinbase/coinbase-advanced-py/blob/master/coinbase/rest/rest_base.py):
  JWT binds method/host/path, while query parameters are passed separately.
- [General CDP JWT authentication](https://docs.cdp.coinbase.com/get-started/authentication/jwt-authentication):
  Ed25519 seed/public-key representation and issuer/audience alternatives.

## Progress

- Fresh branch created; clean landing brief/spec inspected. Independent scope,
  auth-source and public implementation work started. No acceptance gates claimed.

### Public protocol evidence

Read-only live probes on 2026-10-20 resolved conflicting sequence documentation:
`subscriptions:0`, `heartbeats:1`, BTC level2 `2..10`, ETH snapshot `11`,
BTC update `12`, ETH update `13`, subscriptions `14`, BTC/ETH candles `15,16`,
subscriptions `17`, BTC/ETH trades `18,19`, subscriptions `20`, then interleaved
book/trade messages `21..34`. The current AsyncAPI connection-wide rule matches
this observation; the overview's per-product prose does not. Track every valid
connection envelope, and invalidate books on any gap.

The legacy INTX instruments endpoint returned 310 instruments and zero
`TRADING` perpetuals. INTX book subscriptions returned empty snapshots, so the
live public performance fixture uses the supported spot profile and active
BTC-USD/ETH-USD books. Authenticated INTX/CFM/spot coverage uses independent
payload tapes; no private live-auth claim is made without credentials.

Supporting frame change: an optional spot currency table retains total,
available and reserved amounts independently. Legacy aggregate/collateral frames
are unchanged. Engine49 tests and JS wire7 tests pass, including transfer,
reserved holdings, absent-versus-empty and special currency property names.

### Phase 1 checkpoint

Public data and the native signer foundation are implemented. Independent
code-correctness review found sequence-domain, pagination, failed-read metadata,
CFM multiplier, alias snapshot, daily seeding and socket recovery defects. Those
findings are fixed and rechecked. Verification: 16 public tests plus 5 native
signer tests pass; strict venue Clippy passes; bans pass. Native signer cases
independently verify P-256/Ed25519 signatures and reject altered claims.
Full workspace, WASM, SDK and performance acceptance remain later gates.
