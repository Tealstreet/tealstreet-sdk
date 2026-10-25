# Credential selection and nonce authority core

Plan, 2026-10-23. Lane claude-m26ps4 under claude-ddi2il. Pinned landing:
`7641650e7b1` (#1789). Covers architecture audit ranks 5–6: the engine chooses
key slot, signing identity, account subject, scope and nonce once, before
signing; venues declare request semantics and keep protocol codecs only.

Owner constraints carried unchanged:

- Signing stays at send time through a host-supplied signer (local, remote,
  enclave, parent-injected). Remote signing keeps working; the nonce/stamp is
  chosen in Rust before a payload reaches any signer (true today, preserved).
- RUST_AUDITS.md "park read-key selection deduplication until after cutover"
  (Binance re-audit) predates the 2026-10-22 choke-point rule, which lists
  credentials as an engine choke point. claude-ddi2il confirmed (2026-10-23)
  that the newer rule supersedes it; RUST_AUDITS.md is updated.
- RUST_AUDITS.md known nonce limits (restart/cross-process, freshness window)
  stay "document only". This core centralizes reservation; it does not change
  allocation semantics, add cross-process coordination or alter clock sources.
- No key hash, slot name or key reference is ever an identity or nonce-domain
  fallback (RUST_SIGNER_PREREQUISITES.md item 1).

## Inventory (production code at the pin; tests excluded)

### Engine surfaces today

| Surface | Location | Gap |
| --- | --- | --- |
| `CredentialSlot`, `AccountReadScope`, `AuthOutcome`, `MergedScopeAuth`, `Notice::AccountAuth` | `engine/src/engine.rs:160-219,321` | Slot is chosen and tagged by the venue after it has already signed. |
| `Cx::tag_account_read`, `Venue::account_auth_outcome` | `engine.rs:1703-1712,2321-2329` | Opt-in; 4 families tag. Untagged reads produce no evidence. |
| `RequestRoute::Primary` | `engine.rs:78-92` | Venue sets it per request to keep primary-only reads off secondary egress. |
| `request_context::Credential{slot,reference}`, `RoutingIntent` | `request_context.rs:84-125,210-216` | Only Binance passes one; slot comes from venue logic. |
| `signing::{Identity, Identities, CredentialSetupPolicy, KeyRef}` incl. `stable_id` | `signing.rs:14-153` | Only Binance/Bybit sign via KeyRef. `stable_id` has no production reader. |
| `ReadCredentials` raw key/secret/passphrase | `engine/src/credentials.rs:6-42` | Validation only; every venue branches on it locally. |
| `crypto::nonce::{next, ethereum_agent}` | `crypto/src/nonce.rs:13-45` | Process-global monotonic map; callers: HL, TxFlow, Kraken. |
| `entropy` identity/signing domains | `engine/src/entropy.rs:11-27`, `engine.rs:1913-1926` | Correct; random nonces already use signing entropy. |
| `ResourceIds` / `Io::reserve_ids` (#1789) | `engine/src/resource_ids.rs`, `engine.rs:1500` | Per-engine unkeyed counter; not a nonce host. |

### Credential selection, 27 families

Secondary-read support (18 listed; cryptocom accepts but never uses it): asterdex, backpack, binance, bingx, bitget,
bitrue, bitunix, blofin, bybit, coinbase, cryptocom (accepted, unused),
kraken futures, kucoin, mexc, okx, phemex, weex, woo. No secondary: apexomni,
breakoutprop, grvt, hyperliquid, lighter, orderly, paradex, polymarket, txflow,
kraken spot.

Rule patterns (each a local copy of one policy):

1. **GET → secondary, else primary, no exceptions:** asterdex
   `request.rs:198-202`, backpack `account.rs:310-328`, bingx `sign.rs:105-107`,
   bybit `lib.rs:639-643`, mexc `account.rs:135-146`, weex `lib.rs:390-405`,
   bitunix `account.rs:290-302`, binance legacy `lib.rs:413-418`, okx
   `lib.rs:330-344`. **Secondary only when a primary exists:** blofin
   (`account.rs:275-315`), bitget (`account.rs:174-197`), woo
   (`lib.rs:171-183`), bitrue (`lib.rs:307`); kucoin (`lib.rs:265-285`) instead
   falls back to unsigned public headers without a primary (`lib.rs:268,283`).
   Missing primary returns silently in bybit `lib.rs:645`, binance `lib.rs:419`,
   mexc `account.rs:144`.
1a. **Trade-owned GETs:** blofin's replacement-verify order detail is signed
   with the secondary (`trading.rs:636-642,1327-1335`); coinbase's cancellation
   proof and catalogue/trading GETs stay on the trading key (`rest.rs:77-80`).
   Phase 2 lists every non-account signed GET per family with a role.
2. **GET → secondary with named primary-only reads + `RequestRoute::Primary`:**
   bitget degraded balance/UTA (`account.rs:154-173,242-244`), bitrue markets
   (`lib.rs:291-336`), kraken `/api/history/` (`account.rs:389-431`), phemex
   client-id lookup and post-unknown order reads (`lib.rs:200-227`), binance
   and okx client-order-id lookup (`binance/history.rs:497-513`,
   `okx/history.rs:533,562`), coinbase account/history-only reads
   (`rest.rs:77-93,127-129`).
3. **Explicit managed slot:** binance only (`reads.rs:70-96`,
   `identity.rs:89-102`, `trading.rs:474-482,581-583`).
4. **Auth evidence tags + override:** bingx (`account.rs:1680-1753`), mexc
   (`account.rs:175-185,1592-1652`), weex (`lib.rs:432-442`,
   `account.rs:1018-1090`), bitunix (`account.rs:323-349,964-1011`). MergedScope
   in mexc/weex/bitunix, not bingx.
5. **Passphrase falls back to primary:** okx, blofin, bitget, kucoin, weex
   (setup policy `RequiredWithPrimaryFallback` exists but no venue uses it).
6. **Runtime secondary retirement:** coinbase only (drops the read key on 401,
   portfolio mismatch or list failure, `account.rs:849-856,1467-1565`). Others
   attribute the error to the read key and deactivate only on primary refusal
   (bitget, blofin, bybit, okx, mexc).
7. **Identity/subject from primary while reads use secondary:** mexc, weex
   (primary-key fingerprint). Identity GET signed by secondary with no check
   that it is the primary's account: binance, bybit, bitget, bingx, kucoin,
   woo, phemex, backpack. Only coinbase verifies the read key's portfolio.
8. **Venue-chosen account subject (DEX/session):** hyperliquid/txflow
   `vault ?? application_id` + userRole/fee subject (#1701), lighter account
   index candidates, grvt trading account, paradex address+subkey, polymarket
   funder ?? signer, orderly application_id, breakoutprop subaccount, apexomni
   discovered account_id, asterdex v3 user/signer, coinbase portfolio.
9. **WS/session auth:** primary in every family.
10. **Host egress inference (outside venues):** native
    `hosts/native/src/routing.rs:186-222` and TS
    `packages/safe-cex/src/transport/engine.ts:808-821` pick the read egress
    from GET + read-header presence, the TS side with a hard-coded venue list
    (binance, binancepm, binanceinverse, asterdex). TS also keeps a
    `secondaryReadHeaders` map (`engine.ts:82-115`) and, under `readPublic`
    (`:279`, `prepareHttp` `:798-803`), sends unsigned public GETs through the
    read egress for every venue except those four. `RequestRoute` has no
    Secondary variant.
11. **Registry construction:** AsterDEX read-agent exception
    (`registry/src/lib.rs:745-750`), Binance identities (`:1147-1165`). Hosted
    signing facade sets `stable_id: None` (`registry/src/hosted_signing.rs:251,325`,
    `bybit/src/hosted_signing.rs:65`, `binance/src/sign.rs:54`).

### Nonce sources, 27 families

| Pattern | Families |
| --- | --- |
| Shared `crypto::nonce` monotonic | hyperliquid + txflow (one `hyperliquid-agent` domain keyed by agent address digest), kraken (`kraken` domain keyed by **secret digest**, `sign.rs:43-59`) |
| Venue-local static allocator | asterdex V3 (`sign.rs:231-243`, protocol-clock µs floor) |
| Venue-issued counter | lighter (`trading.rs:478-506`, per engine, cleared on sign error/hash mismatch/uncertain submit) |
| Random from signing entropy | bitunix, blofin, coinbase JWT, grvt (u32), polymarket salt |
| Derived from client id | apexomni zkLink (`sign.rs:490-491`) |
| Timestamp chosen at dispatch (protocol clock) | binance, bybit |
| Timestamp chosen in venue code | okx, bitget, bitrue (resend bump), backpack, bingx, kucoin, mexc, orderly, paradex, phemex (expiry), weex, woo, cryptocom (**nonce also used as request id**), polymarket L2, asterdex V1 |
| None | breakoutprop (session) |

Remote signing: SignStep (binance, bybit) prepares the stamp in
`DispatchContext` at send time; Paradex hashes in Rust before
`sign_request`; the hosted signing facade takes the caller's `stamp_ms`. No
signer chooses a nonce today.

## Design

### C1. Credential selection (`engine/src/credential_selection.rs`)

The venue declares a semantic role per request; the engine returns one
immutable `Selection`, used for signing, routing and evidence.

```rust
pub enum CredentialRole {
    Public,
    Read { scope: Option<AccountReadScope> },                     // eligible for the secondary
    PrimaryRead { reason: PrimaryReason, scope: Option<AccountReadScope> },
    Mutation,
    SessionAuth,                                                 // REST login, listenKey, WS auth
}
pub struct SlotPolicy { secondary_requires_primary: bool, public_data_only: bool,
                        no_primary: NoPrimary /* Unavailable | Silent | UnsignedPublic */ }
pub struct Selection { pub slot: CredentialSlot, pub intent: RoutingIntent,
                       pub reference: Option<KeyRef>, secondary_epoch: u32,
                       evidence: Option<AccountReadScope> }
pub struct Slots<T> { primary: Option<T>, secondary: Option<T> } // venue's signers, indexed only
impl Cx { pub fn select_credential(&self, role: CredentialRole) -> Result<Selection, CredentialUnavailable> }
```

Rules reproduce today's behaviour, with per-family `SlotPolicy` data from the
inventory (rule 1 gates, KuCoin's unsigned fallback, silent missing primary):

- `Read` → secondary when configured, not retired and the policy gate holds;
  else primary.
- `PrimaryRead` → primary with intent Primary, always (bitget, bitrue, kraken,
  phemex, binance/okx history set `RequestRoute::Primary` unconditionally).
- Any primary-signed private request carries intent Primary whenever a
  secondary was supplied at construction, retired or not (coinbase
  `read_supplied`, `rest.rs:81-84`).
- `Mutation` / `SessionAuth` → primary. Missing primary keeps the family's
  current `NoPrimary` behaviour; a typed `notSent` is a listed change per PR.
- `Public` → no credential.

HTTP method is never an input, end to end: every credential-bearing
`HttpRequest` (legacy and managed) carries the selection's `RoutingIntent`
(new `RequestRoute::Secondary` or an intent field), and hosts route on intent
only. Phase 2 deletes the native GET/header inference and the TS venue list
(inventory rule 10). Public intent keeps today's `readPublic` public-egress
routing as explicit host data, so public traffic routing does not change. `request_context::Credential.reference` becomes optional so
raw-secret venues get Primary/Secondary intent rather than Public (agreed with
codex-uufub4 before editing `request_context.rs`). Selection
feeds codex-uufub4's `PreflightPolicy` read/write intents unchanged. Equal
slot/ref never implies equal account.

- **Raw and KeyRef venues alike:** `Slots<T>` holds whatever the venue signs
  with (raw `Credentials`, `Identity`, `Signer`); the venue only indexes it with
  `selection.slot`. One engine resolver handles passphrase fallback for raw
  slots (today `ReadCredentials::validate_passphrase`) and KeyRef slots
  (`RequiredWithPrimaryFallback`); an explicitly empty secondary passphrase
  never falls back (`credentials.rs:63-64`, `blofin/account.rs:296-305`).
- **Evidence:** a scoped selection auto-tags `(slot, scope)`; venues stop
  calling `tag_account_read`. The venue codec still classifies native
  success/refusal via `account_auth_outcome`. Secondary evidence applies only
  if its `secondary_epoch` is current; primary evidence is never discarded by
  retirement. History success never heals account auth.
- **Retirement:** `Cx::retire_secondary(cause)` bumps the secondary epoch only;
  later selections fall back to primary. The request context pins the slot
  across physical retries, so a retry of a leg whose secondary epoch is stale
  is denied (typed, not sent) and the next logical read re-selects (tested).
  Generalizes Coinbase's runtime drop; other families adopt it only
  explicitly, listed per PR.

### C1 as built (Phase 2a: core, hosts, Binance)

- `Public` is not a role (an unsigned request has nothing to select), and
  `NoPrimary` is not policy: `select` returns `CredentialUnavailable::Missing`
  and the venue keeps its existing presentation (silent skip, typed failure,
  KuCoin's unsigned fallback). Only `secondary_requires_primary` is family data.
- `RequestRoute::Secondary` exists; `Selection::bind` declares it on
  non-managed requests. Native and TS hosts honour the declared route, and they
  reject a Secondary route with a non-secondary managed intent. The legacy
  GET/header inference stays for unadopted venues and is deleted with the last
  adopter.
- Secondary-epoch evidence gating and `retire_secondary` adoption land with
  Coinbase (Phase 2b), the only family that retires a read key today. The C4
  checker is a separate Phase 2 PR.

### C2. Account subject (decided by Sam 2026-10-23)

`AccountSubject { signer, trading_account, fee_subject, public_id }` per slot,
kept as separate fields, resolved once by the venue identity codec and read by
every request from the engine instead of recomputed. Decision: secondary
account reads require the secondary key's resolved account subject to equal
the primary's. A mismatch is refused with a typed, not-sent error, never
silently served from another account (Coinbase checks this today, by
portfolio). This changes behaviour for misconfigured read keys and gets a
RUST_CUTOVER_CALLERS entry. It ships in its own PR after the Coinbase
adoption.

### C3. Nonce authority (single store, moved into the engine)

`crypto` depends on `safe-cex-engine` (`crypto/Cargo.toml:8`), so the engine
cannot wrap `crypto::nonce`. The one process-global `NONCES` store moves into
the engine in the same PR, and `crypto::nonce::{next, ethereum_agent}` become
thin callers of it. Map keys stay byte-identical (`sha256(lowercase address)`
for `hyperliquid-agent`), so a family still calling the legacy function and an
adopted family share one sequence. A test reserves alternately through both
paths for one agent and asserts strict increase.

```rust
pub struct NonceKey(Kind, [u8; 32]); // built only via the constructors below
impl NonceKey {
    pub fn stable_id(id: &str) -> Self;              // trusted Identity.stable_id, domain-tagged hash
    pub fn ethereum_address(address: &str) -> Self;  // public material only
}
pub enum NonceFloor { RawMs { scale: u64 }, ProtocolMs { scale: u64 }, None }
impl Cx { pub fn reserve_nonce(&mut self, domain: &'static str, key: &NonceKey,
                               floor: NonceFloor, limit: u64) -> Result<u64, NonceError> }
pub struct VenueCounter; // per engine: seed(epoch, validated), take(), invalidate()
```

- No constructor accepts a raw digest, so a secret digest cannot enter. Stable
  ids hash with a tag distinct from public identifiers.
- Domains stay distinct: HL/TxFlow share `hyperliquid-agent` deliberately;
  Kraken and Aster keep their own. Each family keeps its current floor clock;
  clock policy is codex-5c57jl's lane.
- Remote-signed families that need a monotonic domain require a non-secret
  key:
  - `Identity.stable_id`, or
  - a public identifier when the venue's own sequence is public (Aster V3
    signer address, HL agent address).
  
  It is never a hash of key material, and never an address derived from a key
  inside a venue. A family whose only identity is secret material (Kraken)
  gets a typed unavailable when `stable_id` is absent. The hosted signing
  facade's caller-supplied `stamp_ms` stays unchanged.
- Kraken's secret-digest key moves to `stable_id` with its signer migration
  (codex-5c57jl, #1718). Today slots with an equal API key share one sequence
  even when their secrets differ (`kraken/account.rs:403-415`), so trusted
  setup rejects an equal API key carrying different stable ids. The
  equal-key/different-secret test stays.
- Remote/SignStep: `DispatchContext` gains reservation at prepare time. A
  fresh value is reserved only for an attempt proven not sent. A possibly-sent
  attempt never re-reserves: the HL nonce is its replay-dedupe key. API is
  reviewed with codex-5c57jl and codex-uufub4, after socket #1771 (which
  touches `signing_dispatch.rs`), before touching `SignStep` or provider
  schemas. Hosted external wire and remote precedence stay unchanged.
- `VenueCounter` replaces Lighter's `next_nonce` and **stays per engine**; a
  process-shared counter would change allocation, against the owner's
  "document only" decision. Rules:
  - reserve before signing;
  - never re-reserve or replay an uncertain mutation;
  - reseed only from a validated `nextNonce` whose epoch matches the counter
    (drop seeds that predate a reservation or invalidation);
  - keep the 21104 one-shot resync and re-sign after a definitive refusal
    (`trading.rs:762-768`).
- Random/derived/timestamp families keep their protocol data; they are listed
  so a future duplicate-timestamp defect (e.g. cryptocom nonce = request id)
  has one owner.

### C3 as built (part 1: store, keys, Hyperliquid/TxFlow)

- `engine/src/nonce.rs` holds the shared process-global store. AsterDEX V3
  keeps its own static (`asterdex/src/sign.rs:231`) until its migration, after
  its open PRs.
- `crypto::nonce::kraken` is a thin caller of `reserve_kraken_legacy`, which can
  reach only the `kraken` secret-digest domain. It goes away with Kraken's
  stable-id signer migration.
- `NonceKey::{stable_id, ethereum_address}` are the only constructors. The
  `hyperliquid-agent` key stays byte-identical (`sha256` of the lowercase `0x`
  address), so no sequence resets across the upgrade.
- Family policy is declared once in the engine as a `NonceDomain` constant
  (name, raw/protocol clock floor and scale, limit); `HYPERLIQUID_AGENT` is the
  first. `Cx::reserve_nonce(&domain, &key)` computes the floor with exact
  integer arithmetic.
- Hyperliquid and TxFlow reserve through one `reserve_agent_nonce` helper.
  `crypto::nonce::ethereum_agent` is removed.
- Lighter reserves from an engine `VenueCounter`:
  - one per engine, as before. Two engines on one API key still rely on
    Lighter's 21104 resync: sharing the counter would change allocation, and
    the owner ruled nonce limits document-only;
  - it is bounded by the signer's safe-integer range;
  - it is seeded only from a validated `nextNonce` reply whose request epoch is
    still current;
  - `take` reserves before signing;
  - sign errors, refusals, uncertain sends and the 21104 resync invalidate it,
    which bumps the epoch.
- Still to come:
  - Aster V3 (`asterdex-v3`, ProtocolMs x1000), after its open PRs;
  - `DispatchContext::reserve_nonce`, after #1771.

### C4. Enforcement

`scripts/check-credential-selection.py` + legacy-site JSON (same shape as
`check-request-context.py`) bans, outside listed legacy sites:
- venue-side `ReadCredentials`/method credential branches and `tag_account_read`
  calls;
- `crypto::nonce` calls and venue nonce statics;
- host egress inference from method or read headers (native `routing.rs`, TS
  `transport/engine.ts` venue list).

Each adoption PR prunes its own rows.

As built: `sdk-rs/scripts/check-credential-selection.py` +
`credential-legacy-sites.json`, run by `check-bans.sh`. It freezes production
venue sites naming `read_credentials`, `ReadCredentials`, `tag_account_read`,
`ethereum_agent`, `nonce::next` or `NONCES`, plus native `Method::Get` egress
inference and the TS method/venue-list inference. Inline `#[cfg(test)]` modules
and test files are excluded (only the `#[cfg(test)] mod` items themselves).
Counts are per file and term, so neighbouring edits by other lanes never churn
them; `nonce` imports and `as` renames of frozen names fail outright. A rename
still reads as a prune, so reviewers confirm every removed count is a real
adoption. Option fields and setup validation that name
`read_credentials` stay listed: the inventory is frozen, not required to reach
zero.

### C6. Explicit egress for unsigned requests (agreed with codex-uufub4)

Bitrue's keyless catalog must leave via primary egress without a fabricated
credential. The egress is captured at issuance, inside the same registry
transaction, so the registered Context and completion root hold it exactly:
- `Cx::http_context_with_egress(scope, policy, RoutingIntent)` is additive;
  `http_context` passes none.
- A credentialed context's egress must equal its slot's intent.
- An unsigned context accepts only Primary (Public is a no-op; Secondary is
  rejected).
- Legs and retries inherit the egress and cannot widen it.
- `routing_intent()` returns the egress, or the credential-derived intent when
  none was given.

uufub4's `admit_http_command_with_egress` later shares the same private
transaction.

### C5. Out of scope here, owned elsewhere

- WS/session auth credential use: always primary. `SessionAuth` adoption
  follows socket core #1771 (owns socket auth).
- Session-login prerequisite gates (Paradex JWT, GRVT login, BreakoutProp
  login/discover and similar token readiness) belong to claude-55ao7t's
  background-session core. This core supplies only the key and subject a
  session uses (`SessionAuth` selection).
- Server-held key lifetime and teardown (audit §D host lifetime): see
  `codex-server-order-path-audit.md`; this core holds no key material.

## Phases and PRs

Open PRs at the pin, by area:

| Area | PRs |
| --- | --- |
| `engine/src` | #1771 (`engine.rs`, `signing_dispatch.rs`, socket auth), #1747 and #1798 (`engine/src/lib.rs`) |
| Venues | #1771 bitunix/okx; #1773 okx; #1779 bingx; #1747 bingx/weex; #1785-#1788 aster/grvt/txflow/woo/cryptocom/backpack/paradex (#1786 also mexc/phemex/polymarket); #1791 orderly; #1793 mexc; #1794 txflow history; #1795 phemex; #1796 woo; #1797 breakoutprop/hyperliquid/kraken; #1798 bitget/kraken + `engine/src/lib.rs`; #1799 aster/bybit/coinbase; #1800 polymarket |
| Lanes | #1802 BloFin request context (codex-9czdjh; KuCoin, Bitrue, Bitunix queued next); #1718 HMAC signer queue (codex-5c57jl; per-family slots and Kraken aliasing, covers BloFin/KuCoin) |

Re-check before each PR.

1. **This plan** (docs only) + independent review, reported to claude-ddi2il.
2. **C1 core + two adopters + host routing:**
   - Adopters: Binance (managed KeyRef slot; client-id `PrimaryRead`) and
     Coinbase (inverted default, retirement, `read_supplied` route) once #1799
     merges. Coinbase is a genuinely different family.
   - Alternates: BloFin/KuCoin, only by agreement with codex-9czdjh and
     codex-5c57jl.
   - Includes intent-based host routing (native + TS) and the C4 checker.
   - Core conformance fixtures model the hard cases even without adopting those
     venues: Coinbase retirement, Bitget's dynamic degraded balance, KuCoin's
     unsigned fallback, primary-required gates.
   - Before the change, attempt RED for production-impact candidates:
     - POST read signed by secondary but sent via primary egress (and the
       reverse);
     - secondary-signed reads after retirement;
     - evidence from a stale secondary epoch.
     Only proven failures are called regressions; the rest are conformance
     tests.
   - Sequenced after #1771 if it still touches `engine.rs` `Cx` regions.
3. **C3 core + two adopters:** Hyperliquid and TxFlow together
   (`ethereum_address`, shared domain), plus Lighter (`VenueCounter`), after
   #1771.
   - RED candidates: a Lighter seed accepted after a newer reservation;
     reservation after an uncertain submit.
   - Aster's allocator follows when its open PRs land.
4. **Evidence fan-out:** BingX, MEXC, WEEX, Bitunix move tagging into selection
   once #1747/#1771/#1779/#1793 merge.
5. **Selection fan-out:** the remaining secondary-read families, in PRs of 2-3
   grouped by pattern. Each PR lists the per-family role table and any behaviour
   change, including:
   - BloFin's verify GET (preserve as secondary unless decided);
   - missing-primary handling;
   - registry AsterDEX/Binance construction.

   Each PR prunes its checker rows.
6. **C2 subject**: refuse secondary reads whose account subject differs from the primary's (typed notSent); RUST_CUTOVER_CALLERS entry.

Each PR:
- CARGO_BUILD_JOBS=2; `cargo test -p` for touched crates, plus clippy/fmt;
- independent review answers "per-venue fix of a cross-cutting concern?";
- no merge until claude-ddi2il grants a window.
