# HMAC family signer migrations

Planning snapshot: signer PR #1714 at
`0343e497e0cea91d9b1cfaa4e7fe412f0776a714`; venue source checked against that
pin and landing `084af54692420e95fb1f43837c3d6e589c682c4e`. Owner:
codex-5c57jl. This document changes no implementation or migration status.
Core #1714 subsequently merged at `ad0e6fa1e42df883b8fe947e6b4c04eda0da2f7d`;
the shared prerequisites PR remains an implementation gate.

## Entry gate and shared contract

**No family source edits until both #1714 and codex-uufub4's shared prerequisites
PR actually merge.** Re-read their actual APIs/pins before implementation.
The pinned [signer specification](https://github.com/Tealstreet/tealstreet-sdk/blob/0343e497e0cea91d9b1cfaa4e7fe412f0776a714/docs/plans/RUST_SIGNER.md)
and Binance `src/sign.rs::{Http, SpotSubscribe}` are the reference, not a claim
that future APIs already exist.

At the pin, `engine::signing` supplies opaque `Identity { credential, signing }`,
`Present`, `Sign`, `Sha256`, private `PayloadPart::Credential` interpolation and
earlier-result chaining. HMAC operations return raw bytes; recipes encode the
final hex/base64 headers exactly. `SignStep::{prepare, apply, freshness_ms}` and
`Cx::{sign_http, send_signed}` use the shared bounded dispatch service. Stage
unsigned inputs; materialize credentials and signatures atomically into outbound
actions, never venue options/state or logs. Unsigned public I/O bypasses signing;
preserve explicitly authenticated public/token routes described below.

Shared prerequisites, owned/coordinated with codex-uufub4:

1. Opaque passphrase references, fallback rules and trusted setup; the pinned
   Binance adapter currently has only key/secret and erases secondary passphrase.
   Include KuCoin's additional broker-signing reference in the same custody path.
2. Trusted provider key decoding for Kraken; no decoded secret in a venue.
3. Public signed-WS-upgrade staging. Owner sequencing puts transient session-token
   custody/retirement in the later WS-auth core: implement HTTP first; token-bearing
   listen/bullet URLs and subscription-frame migrations wait for that shared core.
4. Engine-injected signing entropy in `SignStep` for BloFin nonces. Preserve the
   21-byte nanoid-style draw and filtering, not a new UUID/randomness source.
5. Trusted caller-assigned stable identity IDs, never key-derived fingerprints.
   Kraken nonce coordination must survive engine recreation/clock regression and
   preserve equal-primary/secondary-key aliasing without exposing either key.
6. Actual dispatch timestamp/expiry receipts: Phemex uncertain-write repair and
   Bitrue timestamp-increment retries need the stamp that was really sent, not a
   staging-time guess. Coordinate with the shared RequestContext/WS-auth work;
   do not add family queues, metadata caches or credential workarounds.

All families also need registry/construction, trusted SDK/native/WASM/Workers
setup and host admission enabled together, as Binance does. One integration owner
edits those shared paths and the signing-site ratchet; family workers own disjoint
venue directories. Remove each migrated family's legacy signing-site entries and
scan its sign module too; do not widen exceptions. Keep required NX trusted
bootstrap/provider adoption explicit in the caller ledger.

Use engine time at dispatch. Clock PR2 supplies calibrated protocol time only to
canonical stamping; host scheduling/deadlines remain raw. Reconcile its actual API
before Bybit adoption: never apply both the old venue offset and calibrated time.
Preserve current offset/resend semantics until that shared change lands.

Client IDs remain immutable unsigned recipe inputs across re-preparation. Keep
`fill_entropy` identity draws separate from BloFin's `fill_signing_entropy`
protocol nonces; no reminting IDs or extra identity draws when signatures refresh.
MEXC alone has the owner-directed public identity transition below. Bybit retains
venue `userID/inviterID` from `/v5/user/query-api`; Bitget retains venue
`userId/parentId/inviterId` from `/api/v2/spot/account/info`. Native OKX/BloFin keep
`AccountLookup::Unsupported`; do not invent an identity endpoint during migration.

## Per-family plans

Paths below are relative to `sdk-rs/venues/`; line numbers describe the snapshot.
Every family includes unsigned public controls, primary/secondary validation,
delayed/refused/malformed/timed-out providers, stale re-preparation, retired-socket
completion fencing and no mutation replay after uncertain transmission.

### Bybit, inverse, spot, demo and HyroTrader

- **Sites/primitive:** `bybit/src/sign.rs:21–54`, REST `lib.rs:641–683`, separate
  opposite-environment probe `948–975`, WS `account.rs:1702–1723`. HMAC-SHA256,
  lowercase hex of `floor(ms) + API-key + recvWindow + payload`; GET signs encoded
  query without `?`, POST exact ordered JSON. Interpolate API key privately.
- **Slots:** ordinary GET/probe uses secondary when configured; writes and WS use
  primary. Preserve selected-slot evidence, proxy origins and Hyro endpoint rules.
- **WS/time:** sign `GET/realtime + floor(ms+10000)`; expiry stays a string, Hyro
  retains `req_id`. REST window defaults to 20000 and clamps 1–20000. Preserve the
  single eligible clock/signature resend and current calibration behavior.
- **Proof:** suite `bybit` has public/private cases for all five names. Add delayed
  REST/probe/WS stamps, escaped-key interpolation, configured windows, Hyro auth
  correlation and no write retry after ambiguous transport; retain account fences.

### OKX, EU, demo and EU-demo

- **Sites/primitive:** `okx/src/sign.rs:11–40`, `lib.rs:322–385,437`,
  `account.rs:271`, `trading.rs:795`. HMAC-SHA256/base64 of
  `timestamp + METHOD + pathWithQuery + exactBody`, before proxy URL rewriting.
- **Slots:** ordinary GET selects secondary key/secret and secondary passphrase
  with primary-passphrase fallback. `history.rs:533` client-ID lookup GET forces
  primary. POST and WS use primary; preserve simulated-trading headers and EU hosts.
- **WS/time:** `account.rs:365–393`, private and business logins; seconds timestamp
  signs `GET/users/self/verify`. REST uses UTC ISO milliseconds from floored ms.
- **Proof:** suite `okx`: global linear/inverse and EU X-perp public fixtures;
  private `okx`, `okxeu`, `okxdemo`, `okxeudemo`. Retain lookup/products/startup-auth
  tests; add delayed REST/WS, passphrase fallback and primary-lookup recipes.

### Bitget, USDC and inverse; Classic and UTA

- **Sites/primitive:** `bitget/src/sign.rs:5–31`, `account.rs:161–207`;
  `identity.rs:107`, `history.rs:415`, `trading.rs:500`. HMAC-SHA256/base64 of
  `timestamp + METHOD + pathWithQuery + exactBody`.
- **Slots:** ordinary GET/history/settings/identity selects secondary plus
  passphrase fallback. Degraded balance/UTA-assets GET explicitly selects primary
  and `RequestRoute::Primary` (`account.rs:153–158,239`). Writes/WS use primary.
- **WS/time:** login `account.rs:280–314` signs seconds + `GET/user/verify`;
  REST stamps floored epoch milliseconds. Preserve UTA promotion and retry policy.
- **Proof:** `bitget` public/private plus private `bitgetinverse`; separate
  `bitgetusdc` public/private suite. Test ordinary-secondary/degraded-primary GET,
  POST and WS delay, all passphrase slots, account joins and auth evidence.

### BloFin

- **Sites/primitive:** `blofin/src/sign.rs:6–24`, headers `account.rs:283–329`,
  `closed.rs:125`, `history.rs:348`, `trading.rs:1212,1338`. HMAC-SHA256 of
  `pathWithQuery + METHOD + timestamp + nonce + exactBody`, encoded as
  **base64 of ASCII lowercase hex**, not base64 of raw MAC.
- **Slots:** GET uses secondary/passphrase fallback, including trade-owned Verify
  GET. POST and WS use primary. Retain joined leverage and partial-effect handling.
- **WS/time:** `account.rs:498–545` signs `GET/users/self/verify`; REST ms is
  floored text, WS timestamp integer. Route nonce generation through injected
  signing entropy, preserving the 21-byte draw/filter and agreed reprepare policy.
- **Proof:** public/private `blofin`; existing encoding vector, secondary tests,
  startup refusal and joined-leg controls. Add nonce draw/order and entropy-failure
  tests, stale async preparation and failure after an earlier accepted leg.

### MEXC

- **Sites/primitive:** `mexc/src/sign.rs:12–56`, REST `account.rs:126–185`,
  WS `328–355`, separate history path `history.rs:322–345`. HMAC-SHA256/hex of
  `API-key + num::format(now) + payload`; **do not floor fractional time**. GET
  sorts/encodes parameters; POST signs exact body. Receive-window header is outside
  the preimage, default/clamp 1–20000.
- **Slots:** ordinary GET uses secondary; private history deliberately uses primary
  despite GET. POST/WS use primary. Preserve trimmed primary signing key versus
  untrimmed secondary key and account-read scope/egress tags.
- **WS/time:** login uses the same preimage with empty payload and string `reqTime`.
  Keep books/trades public and unsigned.
- **Identity/proof:** replace the current original-key hash in `identity.rs` with
  real venue UID when verified, otherwise the setup-assigned stable account ID,
  per owner decision. The current implemented assets response has no UID;
  [official asset-response fields](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/get-all-account-assets)
  likewise contain no account UID (checked 2026-10-22). No generic futures UID read
  was verified; use the fallback, not an invented endpoint or extra permission.
  List persisted-ID mapping/cutover impact for codex-cxxyqr. Suite `mexc` is
  public/private; test fractional stamps, whitespace-key signing, assigned-ID
  stability, primary history versus secondary account, and owned NotSent failures.

### Phemex

- **Sites/primitive:** `phemex/src/sign.rs:10–33`, REST `lib.rs:191`,
  WS `account.rs:681`. HMAC-SHA256/hex of
  `path + percent-decoded query without ? + expiry + exactBody`; literal `+`
  remains `+`. Expiry rounds nearest second, approximately now +60 seconds.
- **Slots:** ordinary GET secondary; definitive history lookup and uncertain-write
  open-order census primary. Writes and WS primary. WS `user.auth` signs private
  API-key interpolation + expiry and presents key/signature/numeric expiry.
- **Time/receipt:** `trading.rs:1105,1499` fences Unknown repair through actual
  signed expiry +5 seconds. Use the shared dispatch receipt after async/stale
  preparation; never retain staging expiry while refreshing the outbound stamp.
- **Proof:** public/private `phemex`; exact sign vectors, escaped client-ID query,
  secondary/definitive-lookup and late-landing tests. Add rounding-boundary delays,
  actual-expiry repair fence and late closed-socket provider completion.

### WOO

- **Sites/primitive:** `woo/src/sign.rs:16,25`, REST `lib.rs:146–190`,
  WS `account.rs:727–761`. HMAC-SHA256/hex: v1 signs sorted RFC3986 form + `|` +
  floored ms; GET sends query, other verbs form body. Other routes sign
  `floor(ms) + METHOD + pathWithQuery + exactBody`, preserving parameter order.
- **Slots:** private GET secondary; writes/WS primary. `/public/` GET bypasses
  signing. WS URL suffix is non-secret `application_id`, **not API key**.
- **WS/time:** auth signs `| + num::format(now)` and sends that same non-floored
  numeric timestamp plus provider-presented primary key. Keep REST/WS rounding
  distinct; preserve subscription/auth deadline ordering.
- **Proof:** public/private `woo`; v1/v3 vectors, secondary and auth-deadline tests.
  Add delayed/fractional WS signing, exact v1 sorting/body, typed signer failure
  without false venue-auth evidence and closed-socket exclusion.

### Bitrue

- **Sites/primitive:** `bitrue/src/sign.rs:8`, staging/materialization
  `lib.rs:269,290`. HMAC-SHA256/hex of
  `timestamp_ms + METHOD + canonicalPathAndQuery + exactBody`; truncate nonnegative
  ms, preserve header-only `recvWindow`, strip the host from absolute listen URLs.
- **Slots:** GET normally secondary, market-catalogue GET explicitly primary;
  writes primary. Preserve selected key, account evidence and egress route.
- **WS/time:** primary-signed listen-key acquisition/keepalive, server token in
  upgrade URL (`account.rs:542,825,1756`); no separate WS HMAC login. Native `-1022`
  re-signs once at `max(now, previousSent+1)`, including fixed/backward clocks.
  Bind previous stamp through the shared receipt/session mechanisms.
- **Proof:** public/private `bitrue`; documented vector, secondary and staged-history
  retry controls. Add async absolute-URL signing, timestamp increment, listen
  POST/PUT, token retirement and no replay of uncertain writes.

### BingX

- **Sites/primitive:** `bingx/src/sign.rs:72–122`, `lib.rs::{issue,resend}`,
  direct `identity.rs:94`. HMAC-SHA256/hex over BTreeMap-sorted RFC3986 unsigned
  query with floored ms. Preserve typed POST JSON and encoded signing bytes;
  other verbs send query parameters.
- **Slots:** GET secondary; other methods, including POST reads, primary. Preserve
  selected-signer auth tagging and metadata publication fences across retries.
- **WS/time:** exact `/openApi/user/auth/userDataStream` is API-key-only: use
  `Present` with no timestamp/MAC. Shared session custody supplies its listen-key
  upgrade URL. Exact signature rejection retries once with fresh preparation.
- **Proof:** public/private `bingx`; sign/listen-key, direct identity, auth-results
  and metadata retry-fence tests. Add delayed typed POST, key-only lifecycle,
  signature refresh, obsolete-token completion and once-only owned failure.

### KuCoin futures and spot

- **Sites/primitive:** `kucoin/src/sign.rs:40`, futures `lib.rs:257,294,333,345`,
  `identity.rs:87`, spot `spot/mod.rs:195,246`. Atomic HMAC-SHA256/base64 operations:
  secret signs `timestamp_ms + METHOD + rawPathAndQuery + exactBody`; secret signs
  selected passphrase; optional broker key signs `timestamp + trimmedPartner +
  selectedAPIKey`. Interpolate passphrase/key privately and retain raw signing
  query versus escaped wire URL.
- **Slots:** private GET secondary with primary-passphrase fallback; writes primary.
  Futures authenticated public/token calls may use primary signatures; spot public
  requests remain unsigned. Provision broker material through trusted opaque refs.
- **WS/time:** primary-signed `bullet-private` yields dynamic endpoint/token upgrade
  URL; public-bullet policy differs by profile. Preserve welcome-before-subscribe,
  shared token retirement and one consistent send-time stamp across all headers.
- **Proof:** separate public/private `kucoin` and `kucoinspot` suites; broker,
  fallback, raw-query, bullet/welcome and erasure tests. Add async atomic-batch
  refusal, Unicode encoding, same-stamp headers and obsolete bullet-response tests.

### Kraken futures and spot

- **Sites/primitive:** `kraken/src/sign.rs:35–90`, futures `account.rs:411`,
  spot `spot/account.rs:231`. Trusted base64-decoded key; composite SHA256 then
  HMAC-SHA512/base64. Futures hashes `encodedForm + nonce + canonicalPath`, strips
  `/derivatives`, and uses sorted RFC1738-style form (`space -> +`, parentheses
  literal). Spot signs `path + SHA256(nonce + exactFormBody)`, preserving input-order
  RFC3986 fields with appended nonce. Use chained raw digest results.
- **Slots/nonces:** futures GET secondary except primary history/streamed history;
  equal primary/read API keys share nonce allocation even with distinct secrets.
  Spot is primary-only. Shared opaque setup IDs/alias metadata replace secret
  hashes; preserve process-wide monotonicity across instances, recreation and clock
  regression. Current lower bound is `floor(ms)*1000`, followed by monotonic advance.
- **WS/time:** futures primary key-only challenge request, then SHA256(challenge)
  signed with HMAC-SHA512 for subscription messages; no cached credential-bearing
  signed challenge. Spot obtains signed `GetWebSocketsToken`, then includes token
  in execution/balance subscription frames. Bind both to socket/session lifetime.
- **Proof:** separate public/private `kraken` and `krakenspot`; official spot vector,
  challenge reconnect, primary-history and equal-key/different-secret nonce tests.
  Add async cross-instance/reprepare uniqueness, malformed trusted decoding, token
  retirement and signer failure without dispatched-write replay.

## Parallel batches and acceptance

After both actual entry pins, re-audit changed APIs and settle shared receipts and
nonce aliasing before affected HTTP adapters. Implement HTTP first, then WS auth;
the WS inventory above remains required follow-on scope. A partial HTTP stage is
not a completed family migration. Before enabling a family's migrated construction
path, all its credential-bearing paths must support opaque custody; do not add a
legacy credential fallback to bridge the WS-auth dependency. Implement disjoint
venue files in parallel batches; one owner serializes registry/SDK/host/ban edits:

| Batch | Parallel HTTP family adapters | Additional HTTP readiness |
| --- | --- | --- |
| A | Bybit, MEXC, Phemex | Protocol-clock handoff, stable IDs and actual-stamp/expiry receipt. |
| B | OKX, Bitget, BloFin | Passphrase fallback and injected nonce entropy. |
| C | WOO, Bitrue, BingX | Retry stamps; WS token lifecycle remains a separate shared dependency. |
| D | KuCoin, Kraken | Broker refs, trusted decoding and shared nonce aliasing. |

Use fresh branches from actual landing, independent per-family correctness and
adversarial review, and explicit ownership coordination with cumulative-fill work
in Bybit/Bitget account files. Shared engine/registry/host/build changes require
real integration and fresh CI; unrelated families need not stack. Every review
answers the architecture gate: **shared signing mechanism, venue canonicalization
only; no per-venue implementation of credential custody/timing/retries**.

Replay source of truth is `sdk-rs/harness/ci-gate-suites.mjs`: 14 suites / 43 cases
for these families at the pin. Keep literal books/tape/alias coverage; private books
is empty. Under immediate local providers/tape clocks require byte-identical MACs,
headers, bodies, WS messages, entropy ordering and full nonmetric outputs; inspect
asynchronous freshness with dedicated regressions. MEXC's owner-directed identity
transition is an explicit caller-visible delta, not a normalization exception.
Do not modify tapes, comparators, budgets or broad known-failure allowances.

Run focused native family + Binance/engine/host checks with four Cargo jobs; CI owns
WASM, size and public timing gates. Authenticate complete current-head archive
provenance and compare full outputs (only top-level metrics excluded), explicitly
classifying any independent TS-reference variation. No partial/failing proof is
called equality. Green CI, independent review, fresh separate revert guard and a
separate exact-head merge precede reporting each actual pin to the overseer and
caller lane. No live capture, production flip or deployment is part of this plan.
