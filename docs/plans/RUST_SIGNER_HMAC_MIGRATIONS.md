# HMAC family signer migrations

Owner: codex-5c57jl. Venue inventories below retain snapshot locations from
#1714; re-verify changed paths on each fresh branch. This document is planning,
not evidence of family migration completion.

## Entry gate and shared contract

Signer #1714 is landed at `ad0e6fa1e42df883b8fe947e6b4c04eda0da2f7d`,
prerequisites #1721 at `802e9c8c6e947aa36864aabb7254aa6e50c92b4e`,
and protocol clock #1722 at `d3e34e6e10523bd3bb465be362e4cc7fc75671b2`.
Shared wire-stamp #1726 is landed at
`e09e34486259f4efb44cc04c84a9eec6f37c5974`; provider feature seam #1731 at
`06b321192a65562db61489b2cbcdcfe4fce5d055`. Family implementation consumes
these actual contracts.
Bybit #1728 merges actual landing after #1731; other families start from the
landed seam and do not stack on Bybit.

The landed signing contract supplies opaque identities containing credential,
signing and optional passphrase references plus trusted stable identity IDs.
Primitive batches support Present, Sign, SHA256 and private credential
interpolation/earlier-result chaining. Canonical recipes encode raw signature
results exactly; venues never retain key, secret or passphrase material.

`SignStep::prepare(outbound, &DispatchContext)` receives protocol UTC
`now_ms`, shared integer `stamp_ms()`, and only injected
`signing_bytes(out)` entropy. `Cx::sign_http`, `send_signed`, and
`connect_signed` stage unsigned work through the bounded dispatcher.
Raw host `Cx.now` controls scheduling, leases and deadlines; protocol time
controls only venue wire fields. Clock revisions invalidate stale preparation
without extending raw operation deadlines. Do not restore venue-local offsets.

Trusted setup owns raw fields, decoding, trimming, passphrase and stable-ID
metadata. SignResult echoes an exact SignReceipt; stamp/expiry fields remain
safe-integer protocol-UTC milliseconds. No-Cx on_signing_receipt runs only
on the final fresh release and cannot become an alternative dispatch/admission
path. Actual request-release scheduling and outcome aggregation belong to the
shared RequestContext workstream.

Shared prerequisites and sequencing:

1. #1721 provides opaque passphrase refs, trusted key decoding, signed upgrade
   staging, signing entropy and setup-assigned stable IDs. Preserve absent
   versus explicitly empty passphrase fallback. No key hashes as identity.
2. Transient listen/bullet/session token custody waits for the WS-auth boundary.
   Static long-lived-key WS HMAC login migrates with HTTP so no raw credentials
   remain in a venue. MEXC uses direct login, not a session-token escape hatch.
3. MEXC requires the separate shared normalization policy PR #1738 (unmerged):
   primary credential trim=true, secondary=false, applied in trusted setup
   consistently across SDK/native/WASM. No trimming inside venue recipes.
4. MEXC physical 450ms pacing starts at actual HTTP release, not when a
   provider is queued. MEXC migration waits for RequestContext; no local
   apply/receipt workaround.
5. Phemex expiry-based repair compares raw scheduling time; protocol-UTC
   signing expiry needs the shared release-deadline projection in RequestContext.
   Phemex waits for it; never mirror clock offsets locally.
6. KuCoin broker refs and Kraken process-wide nonce identity/aliasing must be
   verified against the actual shared provider contracts before adoption.
   A missing shared contract is reported before source edits.

One owner serializes registry/construction, SDK/bootstrap, host and signing
ratchet edits. Families change canonicalization and semantic pending mappings
only; no venue-owned custody, dispatch, retries or lifecycle mechanism. Remove
legacy production signing sites and scan migrated sign modules, retaining only
independent test oracles. Caller docs record actual landed behavior, including
MEXC persisted identity mapping at cutover.

Client IDs remain immutable across re-preparation. Keep identity entropy draws
separate from signing entropy. Bybit retains venue userID/inviterID; Bitget
retains venue userId/parentId/inviterId. OKX/BloFin identity remains unsupported.

## Per-family plans

Paths below are relative to `sdk-rs/venues/`; line numbers describe the snapshot.
Every family includes unsigned public controls, primary/secondary validation,
delayed/refused/malformed/timed-out providers, stale re-preparation, retired-socket
completion fencing and no new/provider-induced mutation retries. Preserve
existing eligible retry policy and conservatively retain Unknown when a prior
attempt reached the venue but its retry cannot be signed.

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
  correlation, no new/provider-induced retries, and conservative Unknown after
  the existing eligible retry cannot sign; retain account fences.

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
  `API-key + integer protocol stamp + payload`; use shared `stamp_ms()`
  (not a local floor). The old fractional wire value is a port artifact; owner
  requires the intended integer-ms wire contract. GET
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
  Without a verified UID source or explicit `localSigner.primary.stableId`,
  legacy flat construction
  remains usable but fetchIdentity returns typed identity-unavailable. Never
  invent an ID or hash a key. List persisted-ID mapping/cutover impact for
  codex-cxxyqr. Suite `mexc` is
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
- **WS/time:** auth signs `| + integer stamp` and sends that same integer
  numeric timestamp plus provider-presented primary key. Use the shared integer wire stamp after #1726, preserving the intended
  integer-ms TS behavior and subscription/auth deadline ordering.
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

After the actual entry pins, re-audit changed APIs and settle shared receipts and
nonce aliasing before affected HTTP adapters. Implement HTTP first, then WS auth;
the WS inventory above remains required follow-on scope. A partial HTTP stage is
not a completed family migration. Before enabling a family's migrated construction
path, all its credential-bearing paths must support opaque custody; do not add a
legacy credential fallback to bridge the WS-auth dependency. Implement disjoint
venue files in parallel batches; one owner serializes registry/SDK/host/ban edits:

| Batch | Parallel HTTP family adapters | Additional HTTP readiness |
| --- | --- | --- |
| A | Bybit, OKX, Bitget | Actual wire stamp and shared provider feature seam. |
| B | MEXC, Phemex, BloFin | MEXC normalization/pacing and Phemex release projection first; BloFin financial/history overlap resolved. |
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
transition and shared integer-stamp correction are explicit behavior deltas, not
normalization exceptions; declare exact replay keys/tape SHA/reasons if needed.
Do not modify tapes, comparators, budgets or broad known-failure allowances.

Run focused native family + Binance/engine/host checks with four Cargo jobs; CI owns
WASM, size and public timing gates. Authenticate complete current-head archive
provenance and compare full outputs (only top-level metrics excluded), explicitly
classifying any independent TS-reference variation. No partial/failing proof is
called equality. Green CI, independent review, fresh separate revert guard and a
separate exact-head merge precede reporting each actual pin to the overseer and
caller lane. No live capture, production flip or deployment is part of this plan.

Ownership coordination: WOO/Bitrue financial #1725 and BloFin financial #1736
(actual `288155e8d5be4e63ebcc77f4916514dd14e01df5`) have landed.
Kraken Spot, then BingX/KuCoin Spot remain held for cumulative-fill slices before
our whole-family signer migrations; keep disjoint account/signing regions
coordinated and integrate actual pins. Bitunix balance scheduler adoption
remains separate and waits for the actual-release scheduler hook.
