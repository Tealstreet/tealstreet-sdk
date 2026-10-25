# Kraken Futures and Spot native private dispatch adoption

Plan-first refresh on actual rewrite tip `35ae6c6b2548c79f426c3f5c4d66e4e2e349a86a`.
Nonce #2093 landed `c5d7c3595af`; recipe #2096 landed `a44553d0404`.
Owner codex-9czdjh; plan/dispatch reviewer codex-uufub4. No production change
at this checkpoint. NX mode 2 remains parked; #2126 remains HELD.

## Inventory contract

The current request inventory is **18**, with **3 physical sends** and **15
completion/conservative-reference occurrences**. The coherent private cutover
proposes 18 -> 0 only after each owner and terminal is genuinely converted;
changing the three funnels alone earns only 18 -> 15. Hash prefixes below
identify the existing singleton/multiplicity rows, never replacement exemptions.

| Request path under venues/kraken/src | Existing hashes (multiplicity) | Required conversion |
| --- | --- | --- |
| account.rs (2) | 34b6cbe60cec, fc8a5b682da0 | signed_request and signed_streamed physical sends |
| spot/account.rs (4) | 29a69618a4d6; 3ca5b7c3e3f6; 6aa843b32406 x2 | private POST plus its completion/reference owners |
| history.rs (5) | c873db6fc77d; f02822ef2a04 x2; da0f2867dd12; 865ee8f27182 | retained history root/pages and completion references |
| trading.rs (5) | 604f28d91adf; ca37246cf805; 06eea309fa05; 9584979d6cce x2 | genuine queued command/step/terminal owners |
| spot/trading.rs (2) | 12b11a734392, 06eea309fa05 | genuine queued command terminals |

The signing inventory has **29** rows: 19 production and 10 secondary-test rows.
Remove production raw Options/Sensitive custody, signer construction/access and
secret eligibility checks: 11 lib.rs rows, four spot/mod.rs rows and one
spot/account.rs row. Preserve the other three native diagnostic strings and
all test assertions; text containing "secret" is not evidence of custody.
Thus the custody-removal projection is 16 rows, not a claim that 29 -> 0.
Delete the now-orphaned production Signer/primitive implementation and direct
crypto dependencies; retain native wire oracles only in test code. Enable
Kraken sign.rs/sign-module scanning with the migrated-family checker treatment;
separately enumerate every remaining diagnostic, encoding/algorithm declaration
and test-oracle row before freezing its reviewed map. No primitive exceptions,
renames or file moves merely to obtain a prune. Replace the eight credential
inventory accesses with retained Slots/Identity selection; report socket-map
changes separately. Foreign manifest bytes/order stay exact.

## Physical funnels and retained authority

| Funnel | Native consumers and required roles |
| --- | --- |
| Futures signed_request | accounts GET/background Read; leveragepreferences GET/command Read; sendorder, cancelorder, batchorder, editorder POST and leveragepreferences PUT/Mutation |
| Futures signed_streamed | openorders/openpositions/leveragepreferences/background Read; fills GET/Read; account-log and orders under /api/history/ use PrimaryRead with History reason |
| Spot send_private POST | Balance/OpenOrders/Primary Read; GetWebSocketsToken/SessionAuth signed Read; AddOrder/AmendOrder/CancelOrder/Primary Mutation |

Futures Secondary requires Primary and applies only to GET outside canonical
/api/history/. Fills must remain Secondary-eligible. Spot is Primary-only.
Use retained AdmittedRead/ReadLeg::queue_with_nonce and the exact identity/claim
constructor; no reselect after identity replacement. Commands keep their real
root and capped lifetime through queue, serial steps and pages. Background calls
get independent contexts; token login uses the real Session, no REST bearer.
Adopt actual dispatcher prepare/Bind/Claim and validate root/current leg before
stream item, status/codec, native reducer, Store publication or completion.
Retire each physical leg before its successor and retire owners exactly once.

Trusted setup installs provider material outside the venue; options retain
metadata/opaque identities only. Wire registry/native/WASM dispatch-signing
feature forwarding; retain dispatch-nonce for both products. Activate Spot's
landed #2078 ValidateOnly row: validate ignored read material first, wipe it,
never install/transmit it. Preserve native primary/read Base64 and keyless,
public-only, read-only precedence/notices. Consume D1's signing_ref() conversion
when #2132 actually lands; do not import its unpublished head.

## Native preservation controls, before source

Commit compiling originals first, retaining the existing 23 opt-in adoption
originals and all #1876 write and #1798 book assertions. Record actual runtime
REDs versus GREENs and honestly gated suffixes; no ignored tests or setup errors
count as a behavioral RED. Make the adoption target default once implemented.

- Capture current-native wire fixtures on both Futures hosts and Spot. At the
  dispatcher-selected exact decimal nonce, compare native oracle and real
  provider output byte-for-byte: method, URL/query, body including None versus
  empty, ordered headers/API key, signature and challenge/subscription frames.
  Recompute HMAC independently with node:crypto and pin literal vectors.
  Futures keeps sorted form, /derivatives stripping and APIKey/Authent/Nonce/
  Content-Type order. Spot keeps insertion-order form with nonce LAST and
  API-Key/API-Sign/Content-Type/Accept order. No production nonce override.
- Cover every funnel/role above, distinct and equal primary/read API keys with
  different signing material, identity replacement after admission, cross-engine
  and cross-product sequence sharing, reversed releases, parked signer and stale
  final Claim: exact canceled preparation, same Attempt, fresh larger nonce and
  newly recomputed MAC. Canonical u64 controls retain the distinction between
  genuine reservations and maximum-value codec/recipe-only proof.
- Genuine capacity/credit reuse, held Prepare/Bind, clock nonfinite/equality/
  MAX_SAFE, expired command and Background/session cleanup, streamed prefix then
  stale end, duplicate/late provider and HTTP results: no late publication,
  provisional effects, extra terminal or leaked request/sign task.
- Preserve cursor limits/order/economics, serial trade ordering, protective amend
  provenance and partial-effect outcome projection. Spot keeps its one invalid-
  signature resend and independent three nontrade transport retries; a new nonce
  never grants write resend authority. Preserve permission-on-write distinctions.
- Replace Futures challenge signing with epoch-bound shared send_signed and the
  landed #2080 Progress contract: native 15s open plus one accepted challenge's
  15s renewal, no early Accepted/Ready, 180s payload watchdog unchanged. Preserve
  all public/private subscription/ACK/book behavior. Spot token Session retires
  on socket epoch close; stale token responses never subscribe.

## Decisions required before activation

1. **Namespace continuity (uufub4):** legacy reservations use a decoded-secret
   digest; dispatch uses a tagged trusted stable subject. Sharing the same map
   is insufficient. Pin a shared continuity mechanism or an explicitly approved
   coordinated transition proving no higher outstanding legacy reservation can
   be overtaken, including still-live legacy engines. Equal API keys must share
   the trusted subject even with different secrets. No secret/key-hash fallback,
   response seed, venue counter or mixed account-only adoption. Separate memories
   and processes remain a deployment coordination boundary.
2. **Native release/owner fit (uufub4):** use genuine NonceUnscheduled admission,
   never a no-op fixture callback. Check serial Mutation/journal/barrier and
   retained queue semantics against landed APIs: mutable scheduler callbacks
   cannot run inside the nonce commit. Any missing shared seam goes to its core
   owner, not a Kraken bypass. Spot's physical 20s is native; proposed Futures
   physical Read/write windows, logical root/serial bounds and typed prerelease
   changes require explicit disposition and CALLERS disclosure before source.
3. **Construction/socket/host fit:** obtain exact setup and whole-family socket
   review before deleting the last native signer. Hosted adapters must forward
   the exact engine-reserved decimal nonce and signed bytes, never re-reserve or
   fall back. Prove the working adapter before enabling bridge dispatch signing;
   do not break existing hosted users or infer native-family proof from the old
   isolated engine+recipe harness. Unsupported mode behavior needs an explicit
   owner ruling rather than an unannounced refusal window.

## Freeze and landing

Plain-merge the actual landing tip before paired measurements. Measure bare tip
and composition, same archive path/toolchain/locked inputs/profiles: Kraken
family and combined, Node and web, hashes and own byte deltas; use current tracked
budgets unchanged. If any own generic engine code is needed, also measure Bybit
and Bitget on that composition. Serial JOBS=4; delete each own scratch target
after measurement, retain sources/logs/receipts. No size estimate is a pass.

After plan disposition: originals, source, scoped Kraken checks/strict Clippy,
required workspace compatibility check, fmt/full bans/ratchets/selection and
preservation receipts; uufub4 dispatch review plus setup/HMAC/socket fit for the
changed surfaces. One freeze push and one PR-triggered run; READY to 8x5tm1 only
when green and reviewed, then exact-tree GO and guarded head-matched landing.
