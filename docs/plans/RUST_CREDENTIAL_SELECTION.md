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
  credentials as an engine choke point. This plan follows the newer rule;
  claude-ddi2il confirms before Phase 1 merges.
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

Secondary-read support (17): asterdex, backpack, binance, bingx, bitget,
bitrue, bitunix, blofin, bybit, coinbase, cryptocom (accepted, unused),
kraken futures, kucoin, mexc, okx, phemex, weex, woo. No secondary: apexomni,
breakoutprop, grvt, hyperliquid, lighter, orderly, paradex, polymarket, txflow,
kraken spot.

Rule patterns (each a local copy of one policy):

1. **GET → secondary, else primary, no exceptions:** asterdex
   `request.rs:198-202`, backpack `account.rs:310-328`, bingx `sign.rs:105-107`,
   blofin `account.rs:291-315`, bybit `lib.rs:639-643`, mexc
   `account.rs:135-146`, weex `lib.rs:390-405`, bitunix `account.rs:290-302`,
   woo `lib.rs:177-183`, kucoin `lib.rs:265-285` (+ requires primary present),
   binance legacy `lib.rs:413-418`, okx `lib.rs:330-344`.
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
    Read(AccountReadScope),      // eligible for the secondary
    PrimaryRead(PrimaryReason),  // client-id lookup, post-unknown read, degraded balance, catalog, history
    Mutation,
    SessionAuth,                 // REST login, listenKey, WS auth
}
pub struct Selection { pub slot: CredentialSlot, pub reference: Option<KeyRef>,
                       pub route: Option<RequestRoute>, generation: u32, evidence: Option<AccountReadScope> }
pub struct Slots<T> { primary: Option<T>, secondary: Option<T> } // venue's signers, indexed only
impl Cx { pub fn select_credential(&self, role: CredentialRole) -> Result<Selection, CredentialUnavailable> }
```

Rules (exactly today's majority behaviour, now in one place): `Read` → secondary
when configured and not retired, else primary. `PrimaryRead` → primary, with
`RequestRoute::Primary` whenever a secondary is configured. `Mutation` /
`SessionAuth` → primary; missing primary is a typed `notSent` failure. `Public`
→ no credential. HTTP method is never an input, so POST reads and GET
mutations are declared, not inferred. Selection feeds the existing
`request_context::Credential{slot,reference}` (codex-uufub4's
`PreflightPolicy` read/write intents consume it unchanged) and the legacy
signed path alike. Equal slot/ref never implies equal account.

- **Raw and KeyRef venues alike:** `Slots<T>` holds whatever the venue signs
  with (raw `Credentials`, `Identity`, `Signer`); the venue only indexes it with
  `selection.slot`. Passphrase fallback resolves once at construction through
  `SetupFieldPolicy::RequiredWithPrimaryFallback`.
- **Evidence:** a `Read` selection auto-tags `(slot, scope)`; venues stop
  calling `tag_account_read`. The venue codec still classifies native
  success/refusal via `account_auth_outcome`. Evidence applies only if its
  selection generation is current; history success never heals account auth.
- **Retirement:** `Cx::retire_secondary(cause)` bumps the generation; later
  selections fall back to primary. This generalizes Coinbase's runtime drop and
  gives the other families the same transition point (behaviour change per
  venue only by explicit adoption, listed per PR).

### C2. Account subject (decision required before implementation)

`AccountSubject { signer, trading_account, fee_subject, public_id }` per slot,
kept as separate fields, resolved once by the venue identity codec and read by
every request from the engine instead of recomputed. Candidate rule: secondary
account reads are admitted only once the secondary's resolved trading account
equals the primary's, where the venue can report it (Coinbase does this today);
otherwise current behaviour. This changes behaviour for misconfigured read keys
and needs a claude-ddi2il/owner decision. Not part of Phase 1.

### C3. Nonce authority (`engine` wrapper over `crypto::nonce`)

```rust
pub enum NonceKey { StableId(String), PublicIdentifier([u8; 32]) } // never a secret digest
pub enum NonceFloor { RawMs { scale: u64 }, ProtocolMs { scale: u64 }, None }
impl Cx { pub fn reserve_nonce(&mut self, domain: &'static str, key: &NonceKey,
                               floor: NonceFloor, limit: u64) -> Result<u64, NonceError> }
pub struct VenueCounter; // seed(validated), take() -> reserve before signing, invalidate()
```

- Same process-global monotonic store as today; domains stay distinct (HL/TxFlow
  share `hyperliquid-agent` deliberately; Kraken and Aster keep their own).
  Each family keeps its current floor clock (no clock-source change here;
  clock policy is codex-5c57jl's lane).
- `StableId` comes from trusted `Identity.stable_id`; remote-signed families
  that need a monotonic domain require it (typed unavailable when absent).
  `PublicIdentifier` covers protocol public ids (HL agent address).
  Kraken's secret-digest key moves to `StableId` with its signer migration
  (codex-5c57jl queue); this core supplies the domain, aliasing equal
  primary/read credentials by stable id.
- Remote/SignStep: `DispatchContext` gains the reservation at prepare time, so
  every (re)preparation reserves a fresh value and no signer picks one. API is
  reviewed with codex-5c57jl and codex-uufub4 before touching `SignStep` or
  provider schemas; hosted external wire and remote precedence unchanged.
- `VenueCounter` replaces Lighter's per-engine `next_nonce`: reserve before
  signing, never re-reserve or replay an uncertain mutation, reseed only from a
  validated `nextNonce`, keyed by (chain, account index, API key index).
- Random/derived/timestamp families keep their protocol data; they are listed
  so a future duplicate-timestamp defect (e.g. cryptocom nonce = request id)
  has one owner.

### C4. Enforcement

`scripts/check-credential-selection.py` + legacy-site JSON (same shape as
`check-request-context.py`): bans new venue-side `ReadCredentials` method
branches, `tag_account_read` calls, `crypto::nonce` calls and venue nonce
statics outside the listed legacy sites; each adoption PR prunes its own rows.

## Phases and PRs

Venue order avoids files under open PRs at the pin (#1771 bitunix/okx, #1773
okx, #1779 bingx, #1747 bingx/weex, #1785-1788 aster/grvt/txflow/woo/cryptocom/
backpack/paradex, #1793 mexc, #1795 phemex, #1797/#1798 hyperliquid/kraken,
#1799 aster/bybit/coinbase, #1800 polymarket). Re-check before each PR.

1. **This plan** (docs only) + independent review, reported to claude-ddi2il.
2. **C1 core + two adopters:** Binance (managed KeyRef slot, client-id
   `PrimaryRead`) and BloFin (raw GET rule, passphrase fallback), with KuCoin
   as the alternate. Direct core tests: role matrix, POST read, GET mutation,
   missing primary, retired secondary, stale-generation evidence, route
   preservation. Before the change, attempt RED for production-impact
   candidates: secondary-signed reads after retirement, evidence from a
   rotated/stale selection, POST read signed by primary. Only proven failures are
   called regressions; the rest are conformance tests. Adds C4 checker.
3. **C3 core + two adopters:** Hyperliquid (`PublicIdentifier`, shared domain
   with TxFlow unchanged) and Lighter (`VenueCounter`), after #1797 merges.
   RED candidates: Lighter counter lost on engine recreation while venue state
   persists; reservation after uncertain submit. Aster's allocator follows when
   its open PRs land.
4. **Evidence fan-out:** BingX, MEXC, WEEX, Bitunix move tagging into selection
   once #1747/#1771/#1779/#1793 merge.
5. **Selection fan-out:** remaining secondary-read families in PRs of 2-3
   grouped by pattern; each prunes its checker rows.
6. **C2 subject**, only after a decision.

Each PR: CARGO_BUILD_JOBS=2, `cargo test -p` touched crates plus clippy/fmt;
independent review answers "per-venue fix of a cross-cutting concern?"; no merge
until claude-ddi2il grants a window.
