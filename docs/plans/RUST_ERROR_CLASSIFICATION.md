# Engine error classification (one choke point)

Plan and inventory, 2026-10-23, lane claude-dazzy3 (overseer claude-ddi2il).
Base: `feat/rust-rewrite` 7641650e7b1. Register row: "Error classification /
notSent vs unknown — P0" (architecture audit rank 3). Builds on
[RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md) (lifecycle, `CommandWrites`
sealing, `ResponseProof` 408/5xx guard from #1741) and
[RUST_ERROR_HANDLING.md](RUST_ERROR_HANDLING.md) (`VenueError`/`ErrorKind`/`Outcome`).

## Problem

`VenueError`, `ErrorKind` and `Outcome` are shared, but all 27 venue families
decide them locally. Each family copies the same generic layer:
- an HTTP status ladder (about 20 copies, and they disagree on 408, 429, 405 and 425);
- the rules "2xx but undecodable means unknown" and "transport loss on a write means unknown";
- the `not_sent`/`unknown` constructors;
- the rule "a step took effect, so drop Rejected/NotSent";
- the rule "an Unknown leg wins".

The only engine-owned rule is the 408/5xx guard. It applies only to managed
writes, and only Binance has those today, so for 26 families the local ladder
is the only protection. Native code tables exist in about 18 families and are
duplicated, with divergence, between write, read, startup and WS-login paths.

## Contract (engine-owned, in precedence order)

1. **Not released → `notSent`.** Covers preparation, signing, admission and
   queue refusals (`request_context::Error::venue_error()`, #1808, and
   `SignError::venue_error()`). This is a local-operation fact. The mutation
   journal still recomputes the command outcome from earlier releases.
2. **Released, then transport failure → write `unknown` / `network`.** A read gets
   `network`, and read-retry eligibility applies only under `RetryPolicy::ReadFailure`.
3. **408 or ≥500 → write `unknown`** (`timeout` / `venueUnavailable`), whatever
   the body says. A native code in that body is kept only as
   `venueCode`/`venueMessage` evidence and never makes the write `rejected`. On
   the managed path the message stays the engine's "HTTP response does not prove
   the write outcome" (`http_evidence.rs:36`, already live for Binance). There is
   no documented-row escape.
4. **A decoded native error code → venue table verdict, overriding any
   status-derived kind** (KuCoin returns 400002 with HTTP 401, for example).
   Integer codes decode strictly: an exact integer or a canonical quoted
   integer, never `f64`. String codes compare exactly.
5. **Non-2xx with no decodable native error → the venue's documented status row,
   else `unknown` with a status-derived kind** (401 `authInvalid`, 403
   `authPermission`, 418/429 `rateLimited`). "HTTP status alone is not refusal
   evidence" unless the venue documents it.
   - Phase 2 starts by listing each family's documented 4xx semantics, with a
     citation for every status row. Binance, for example, documents 4XX as
     client-side refusals and 418/429 as rate-limit refusals, so those rows are
     `Refused`.
   - A family without documentation flips from today's bare-4xx `rejected` to
     `unknown`. That flip is listed per family in Callers.
   - An `unknown` carrying an auth or rate-limit kind must not fan out reconcile
     reads or hold order locks past the rate window. The engine defers both by its
     own bounded window: the Retry-After value when present, otherwise a fixed
     engine default (10 s, capped at the existing 1 h maximum), and then until the
     next admitted read. This deferral is internal; it never invents a
     caller-visible `retryAfterMs`.
6. **2xx whose success envelope or ACK is malformed, incomplete or count-mismatched →
   write `unknown` / `internal`.** The engine enforces the count: each leg
   declares its expected native-ID count at registration, and an `Accepted` with
   the wrong count is recorded as `Failed`/`unknown`. The engine derives a minimum
   from the intent (a Place leg needs at least one ID per caller order), so a
   venue cannot declare 0 to make an empty ACK pass. For a read, a malformed
   reply is an `internal` failure.
7. **`Satisfied` → `Accepted { order_ids: [] }` for one cancel, amend or setting
   intent only.** It is never a placement and never a read. It proves "not open
   or already in that state", not a store transition. For example, Binance −2011
   on cancel may mean filled or never existed, so the engine marks the order for
   reconciliation and never writes `Cancelled` on that evidence.
8. **One-shot re-sign resend, only through an opaque `ResendGrant`.**
   - The grant comes from `http_write_failure`, and only for a `RefusedResend`
     verdict under rules 3–4 (status <500 and ≠408).
   - `CommandWrites::retry` and `Context::retry` require the grant. Constructing
     `RetryEvidence::DefinitiveRefusal` publicly is removed.
   - Each leg keeps a `resend_used` flag, so a second resend is refused.
   - A resend never comes from message text or engine-formatted strings.
9. **Fact recording and aggregation are engine-only.**
   - `Cx::http_write_result` accepts only `Accepted`. `Rejected`, `KnownPartial`
     and `Failed` are constructed only inside the engine, from a verdict (rules 3–8).
   - An attempt that was never released, such as a Binance signing failure
     (`trading.rs:952`), is recorded through `cx.http_write_not_sent(&context,
     error)`. The engine refuses it with `Error::State` once the attempt has
     been released.
   - Batch ACKs go through `http_write_batch`. The engine computes per-item
     verdicts, the accepted-ID join, KnownPartial versus Rejected, and the count check.
     - Items must arrive in caller-order position. The first non-accepted position
       becomes `failed_index`, and the engine rejects an item count that differs
       from `expected`.
     - A cancel row counts as proven only when it echoes the requested native ID.
       Its ID then joins `order_ids` as cancellation evidence.
   - `CommandWrites::fail` keeps the `notSent`/`rejected`/`unknown`/partial
     projections. Venues never pick the command outcome or demote it after an effect.
10. **Reads never claim money effects.** `ReadFailure` carries a kind, native
    evidence and retry facts, and no `Outcome`.
    - Native retry facts (`RefusedResend`, a rate-limit delay) are separate from
      the signature and transport retry budgets.
    - Code 0 or a 2xx status never proves a successful body or list; success proof
      stays with the venue codec.
    - Auth is derived once, from the table, inside the existing
      `Venue::account_auth_outcome` hook, through `classify::auth_outcome(table,
      native)`. The engine consults that hook only for credentialed account
      reads (`PendingHttp.auth`, set by `tag_account_read`), so public reads
      never produce credential evidence, even with auth-looking codes such as
      Crypto.com 40101. Venues must not call `auth_outcome` outside the hook.
      Managed reads must keep the same gate when they adopt the hook.
    - Read retry: a rate limit waits for its Retry-After, or else the engine
      default (10 s). An auth refusal never retries. A proven `RefusedResend`
      resends once. Any other unproven answer is a read failure, which honours a
      Retry-After on any status.

`kind` comes from the verdict. `retryAfterMs` is set only when a Retry-After header
is actually present. A new `startup::retry_after_ms(&Response) -> Option<f64>`
works for any status, so a native `rateLimited` code on a 200 or 400 can use it.
Unlike `rate_limit_pause_ms`, it never substitutes a default. Venue-specific
weight/limit headers belong to the rate-limit core (audit rank 10), not to this
lane. Outside rule 3, `message` keeps the venue's current text, and the native
code and message go in `venueCode`/`venueMessage`.

The engine reaction to an `unknown` seal also replaces the per-venue
uncertain-write reconcile triggers: OKX `trade_lost`, Lighter submission
uncertainty, and the Orderly −1007 gap (#24). It reuses the existing
order-lock/reconcile fence. This needs coordinating with codex-uufub4 and is
scheduled as phase 2b. Design sketch: on an `unknown` mutation seal the engine
already keeps the order-lock fence (`finish_command` skips `managed_resolved`).
It will additionally call one new hook, `Venue::reconcile_uncertain(symbols)`, where `symbols` is a list or `None` for a full read (cancel-all, multi-symbol commands),
once per seal, after the rule 5 deferral. The venue implements only the read. A
fresh read that `orders_reconciled` accepts clears the fence. The detailed design
is reviewed with codex-uufub4 before phase 2b.

## Code-table API (published to venue lanes before implementation)

Module `engine/src/classify.rs`, re-exported as `safe_cex_engine::classify`.
Tables are `const` struct literals: venue data with no logic.

```rust
pub enum Verdict {
    Refused(ErrorKind),          // proven pre-execution refusal: write `rejected`
    RefusedResend(ErrorKind),    // proven refusal; yields one ResendGrant
    Unknown(Option<ErrorKind>),  // code does not prove the outcome: write `unknown`
    Satisfied(SatisfiedIntent),  // rule 7; requires Row.only == that intent
}
pub enum SatisfiedIntent { Cancel, Amend, Setting }
pub enum Intent { Place, Cancel, Amend, Setting, Read }
pub enum Key { Int(i64), Range(i64, i64), Text(&'static str), MessagePrefix(&'static str) }
pub struct Row { pub key: Key, pub verdict: Verdict, pub only: Option<Intent> }
pub struct CodeTable {
    pub rows: &'static [Row],
    pub unlisted: Verdict,                  // required; never Satisfied
    pub status: &'static [(u16, Verdict)],  // documented 4xx only (rule 5); never Satisfied
}
pub enum NativeCode<'a> { Int(i64), Text(&'a str), None } // None: code-less venues
pub struct Native<'a> { pub code: NativeCode<'a>, pub message: &'a str }
pub struct ResendGrant { /* private */ }

impl CodeTable {
    pub fn verdict(&self, native: &Native<'_>, intent: Intent) -> Verdict; // WS login, startup
}
pub fn decode_int(token: &str) -> Option<i64>;            // strict integer, rule 4
pub fn auth_outcome(table: &CodeTable, native: &Native<'_>)
    -> Option<AuthOutcome>;          // rule 10, only inside account_auth_outcome
pub fn assert_table(table: &CodeTable);                   // venue table tests

// Managed path (Cx). Each call records the fact in the command journal.
cx.http_write_failure(&context, &TABLE, intent, &response, native)
    -> (Verdict, Option<ResendGrant>);
cx.http_write_unproven(&context, message);                // rule 6
cx.http_write_not_sent(&context, error);                  // unreleased attempt only
cx.http_write_batch(&context, &TABLE, intent, expected,
    items: impl Iterator<Item = Result<&str /* native id */, Native<'_>>>) -> BatchVerdict;
cx.http_read_failure(&context, &TABLE, &response, native) -> ReadFailure;
pub struct ReadFailure { pub error: VenueError, pub retry: Option<ReadRetry> } // no Outcome
pub enum ReadRetry { NativeResend(ResendGrant), After(f64 /* ms */), ReadFailure }

// Legacy, unmanaged venues (transitional; see below):
pub enum Released<'a> { No, Transport, Status(&'a Response<'a>) }
pub fn write(table: &CodeTable, intent: Intent, released: Released<'_>, native: Option<Native<'_>>)
    -> (Verdict, VenueError);
```

- **Row lookup order:** the first row whose key matches and whose `only` equals
  the intent, then the first matching row with `only: None`, then `unlisted`.
  Duplicate keys are allowed only when `only` partitions them. For example,
  Binance −2011 is `Satisfied(Cancel)` with `only: Cancel`, plus a refusal for
  every other intent.
- **`assert_table`** rejects:
  - overlapping ranges;
  - unpartitioned duplicates;
  - `Satisfied` in `unlisted`, in status rows, on a `Range` key, or without a matching `only`;
  - status rows for 408 or ≥500.
- **`MessagePrefix`** applies only to the venue's own native message field, and
  only for venues with no native codes (Hyperliquid, TxFlow, Polymarket). It is an
  exact prefix, never `contains`. Each row cites the doc or tape that proves the
  text is stable.
- **One table per venue family** serves writes, reads, startup, WS login and
  `account_auth_outcome`.
- **Legacy `classify::write`** is a pure table lookup. It returns the `Verdict` so
  that a legacy resend can be gated, and it takes a typed `Released` so that rule 1
  and rule 2 stay distinct.
  - Legacy signer failures must route through `on_sign_error`, not
    `on_request_failed(&str)` (#26).
  - `Seal::Legacy` still passes the venue's aggregation through, so this path
    alone never closes a P0 row. P0 rows close only with RequestContext write adoption.
  - The legacy path issues no `ResendGrant`. The legacy resend rows (#5–#8b)
    therefore close only by adoption, and they are listed as adoption blockers
    for their families.
  - The legacy path is deleted with the transitional bridge, as a master-merge
    blocker (RUST_MASTER_MERGE_BLOCKERS.md).

Venue lanes consume this API in their adoption PRs. Breaking changes found in
phase 2 review are announced to codex-9czdjh, codex-zs5e83, codex-ih35js and
codex-hcmq3f before merge.

## Beta classification (2026-10-28)

Lane claude-dazzy3, approved by claude-ddi2il. Base: master `bbffd5e2e42`.
Scope: the 15 beta families in `BETA_VENUES`
(`packages/safe-cex/src/exchanges/rust/betaVenues.ts`, 27 keys): Binance (4),
Bybit (4), OKX (4), Bitget (2), Kraken (2), Hyperliquid (2), Lighter, Paradex,
Bitunix, Phemex, BingX, ApeX Omni, BloFin, Crypto.com and AsterDEX. This
section supersedes #2237. The all-family inventory below is kept as the
pre-beta record. Its beta rows are re-verified here, and its non-beta rows
stay as #2237 left them.

### Categories callers see

Every beta venue maps HTTP status, native codes and WS rejects into these
categories, through its one `CodeTable`. No venue picks a kind outside it.

| Category | `ErrorKind` |
| --- | --- |
| auth | `authInvalid`, `authExpired`, `authPermission` |
| rate limit | `rateLimited` |
| insufficient balance | `insufficientBalance` (new, PR2) |
| invalid order | `invalidInput` (bad price, size, notional, post-only, reduce-only, unknown symbol, local order validation) |
| venue unavailable | `venueUnavailable`, `timeout`, `network` |
| rejected | `rejected`: a definitive venue refusal of another kind (risk limit, position mode or state) |
| unknown | no kind, or `internal` |

Rulings (claude-ddi2il, 2026-10-28):
- **R1.** Add only `ErrorKind::InsufficientBalance`. `invalidInput` is the
  invalid-order class; there is no `InvalidOrder` rename.
- **R2.** A WS order reject keeps its store status. Its kind rides on the
  `ORDER_REJECTED` notice. Changing order-store semantics is out of scope.
- **R3.** `Refused` stays only where the venue documents a coded or 4xx reply
  as pre-execution. Flipping a write from `Refused` to `Unknown` changes caller
  behaviour (the write may have been sent, so the caller reconciles instead of
  treating it as `NotSent`). Each family PR lists every flip with its doc
  citation, and its caller fragment states the change.
- **R4.**
  - Recorded payloads that need no credentials are approved: public endpoints
    with bad parameters or invalid symbols, and bogus-key auth replies. Keep
    them few and paced, never in a loop.
  - Credentialed captures wait for funded test keys, which will come through the
    live-smoke handoff, peek only. Each needs claude-ddi2il's approval of a
    per-venue capture list first.
  - An insufficient-balance probe is a post-only order far from the market,
    sized above the balance, cancelled at once, followed by an open-orders check.
  - Payloads taken from documentation are allowed only for rate limit, 5xx and
    maintenance.
- **R5.** The production TS text-pattern bugs (listed below) get their own PR
  right after PR2. PR2 stays bridge-only.

Engine policy is decided once, from the table `Verdict` plus kind:
- `Refused(InsufficientBalance | InvalidInput)` is a definitive refusal and is
  never retried.
- An auth kind is never retried, and it counts one auth failure.
- `rateLimited` takes the rule 5 deferral.
- 408 and 5xx stay `unknown` whatever the body says (rule 3).

One table per family serves writes, reads (`classify::read`), startup, WS
login and WS order-reject reasons (intent `Place`).

### Beta inventory (master `bbffd5e2e42`)

Paths are relative to `sdk-rs/venues/<venue>/src/`.

**Insufficient balance has no kind in any family.** The codes below are
`Refused(Rejected)` today unless noted. Each family PR re-points them to
`InsufficientBalance`.

| Family | Insufficient balance or margin today |
| --- | --- |
| binance | -2010, -2019 (via `unlisted`) |
| bybit | 110007, 170131 |
| okx | 51008 (range 51000-51999) |
| bitget | 40762 (via `unlisted`) |
| kraken | futures `insufficientFunds`, `insufficientAvailableFunds` (`status_failure`); spot `EOrder:Insufficient funds` (local ladder) |
| hyperliquid | "Insufficient margin to place order." as a batch row only; a top-level err is `Unknown` (`trading/custody.rs:400-416`) |
| lighter | none in the table; only the WS `RejectReason::InsufficientBalance` (`account.rs:1686-1696`) |
| paradex | `INSUFFICIENT_MARGIN` has no kind on a 2xx (no table) |
| bitunix | 20003 |
| phemex | 11001 (`Refused` only for Place on a 2xx; otherwise unproven) |
| bingx | 101204 (Place only) |
| apexomni | none (no balance or order rows) |
| blofin | 103003, 150004 |
| cryptocom | 306 |
| asterdex | -2019, -2010 (via `unlisted`) |

**Invalid-order codes are split.** Some are `InvalidInput`: Binance and AsterDEX
-1199..-1100 and -4999..-4000, OKX 51000/50014/50016, Bybit 10001, BloFin 152xxx,
BingX 100400/80014, and Bitunix 10002/10008/10010/10011. The rest are
`Rejected` or unlisted, and Kraken never uses `InvalidInput`.

**Visible kind decided outside the table** (each is deleted in its family PR):

| Family | Site | Effect |
| --- | --- | --- |
| okx | `trading.rs:1434-1441` `rejection()` | Rebuilds the kind locally: 50101/50103-50105, 50100/50030 and 50016 reach callers as `Rejected`. |
| bitget | `trading.rs:2319-2329` | Status-only ladder: 40001 or 40014 on a 400 reach callers as `Rejected`. Batch failures pass `NativeCode::None` (`:2507-2512`). |
| bitunix | `error.rs:55-130` | Hand copy of `codes::BITUNIX` plus a status ladder. A 4xx with 10003 is `Rejected`, 10007 is not resent, and code 0 is `Internal`. `recovery.rs:185-187` bypasses `auth_outcome`. |
| asterdex | `trading.rs:1471-1489` `failure()` | Second copy of `ASTER`, used for reads and the job error. Cancel -2011 with "unknown order" becomes success by text (`:793-803`). |
| apexomni | `trading.rs:1462-1481` | Local status ladder. The same input classifies differently on write and read (20016, code-less 400, code-less 403). |
| hyperliquid | `trading.rs:966-975`; `trading/schedule.rs:93-111` | Two local ladders. A 5xx is unknown with no kind on orders but `venueUnavailable` on scheduleCancel. |
| kraken | futures `trading.rs:1166-1207` `status_failure`; spot `spot/trading.rs:500-536` | Spot has no table. Its prefix ladder is reused by reads (`spot/account.rs:690-711`). A nonce error is `Rejected` on futures but `authInvalid` on spot. |
| paradex | `trading.rs:1561-1574` `answer_failure`; `depth_query.rs:140-144` | No table. Batch refusals have no kind (`trading.rs:928-934`). 401/403 are `venueUnavailable` on depth reads. |
| lighter | `trading.rs:1959-1968` | One catch-all `Range(0, MAX)` row. With 405/429, code 23000 is `rateLimited` locally but `Refused(Rejected)` in the fact (`:1138-1151`). |
| cryptocom | `depth_query.rs:150-160`; `public_trades.rs:18-27` | Second ladder and a one-row second table. 40801 is `Unknown(None)` in `ERRORS` but `timeout` on depth. |
| blofin | `account.rs:2254-2261` `key_rejection`; `account.rs:404,1334,1343` | Separate auth map with no 401 row. Body substring "152401"/"152406". |
| bybit | `lib.rs:352,1190-1194`; `refresh.rs:306-311`; `lib.rs:403-406` | Startup ladder. Refresh maps any error to `rateLimited`/`venueUnavailable`. A missing `retCode` leaks the HTTP status as `venue_code`. |
| bingx | `trading.rs:10` `KEY_REFUSALS`; `lib.rs:88-94` | Read-side auth list (kept in line by `context.rs:205`). Signature detected by message text. |
| binance | `account.rs:628-692`, `history.rs:989-992` | Reads carry no kind: -2015 is `authInvalid` on writes but a kindless retry on account reads. No startup credential path. |
| phemex | `lib.rs:762-775`; `trading.rs:1678-1682` | Reads are plain strings. "Suspended/PendingAllocate" is text, giving `Unknown(Internal)`. No status rows, so a coded 401/403 has no auth kind. |

**Reads mostly carry no kind.** Account or background reads in Binance, Bybit,
OKX, Bitget, Kraken futures, BingX, BloFin, ApeX, AsterDEX, Phemex and Crypto.com
(account) build plain strings. Each family PR routes them through
`classify::read`.

**WS order rejects drop the reason.** REJECTED or `order_failed` becomes
`Canceled` with the reason dropped in BingX (`account.rs:1715`), BloFin
(`account.rs:834`), Crypto.com (`account.rs:158`), AsterDEX (`account.rs:296`)
and Binance (`account.rs:1943`, post-only by text). Bybit (`account.rs:2163`),
ApeX (`account.rs:1733-1743`) and Paradex (`account.rs:1868-1874`) emit raw
text, and Hyperliquid `*Rejected` order statuses reach only orphan protective
legs (`account.rs:2418-2432`). Under R2 each PR classifies the reason with the
family table and puts the kind on the notice.

**WS login and subscribe failures:**
- OKX maps codes other than 60005/60024/60032 to `internal`
  (`account.rs:2245,2264`).
- Kraken matches "authentication" text (`account.rs:702-710`).
- Bitget, Bybit, ApeX, Crypto.com and Paradex emit kindless notices.

**Message text still classifies:**
- BingX: signature and listen-key text.
- ApeX: "sign".
- BloFin: a body substring.
- Crypto.com: `contains("40101")`.
- AsterDEX: "unknown order", "Timestamp" and builder text.
- Kraken: spot prefixes and `contains`.
- Binance: POST_ONLY/GTX/MAKER.
- Lighter: "market"/"invalid param".

Each is replaced by a code row, or by an exact `Message` row with a cited
reason.

**Tests.** No beta family has a recorded error payload. Every pinned payload is
hand-written inline, and some are synthetic: Phemex and ApeX `10003
"insufficient funds"`, and Lighter "native throttle".

**TS (production path, `packages/safe-cex/src/health/errorPatterns.ts`, text only).**
The legacy adapters are test-only references under
`packages/safe-cex/src/__tests__/reference/`.
- Wrong today:
  - Binance -1022 maps to time sync; it is auth (resend).
  - OKX 50112 maps to auth; it is a timestamp error.
  - OKX 50100, 50101 and 50103-50105 are missing.
  - The Bybit auth streak counts 10005.
  - Bare `429`/`502`/`503` substrings match numbers inside messages.
- Missing: there are no Bitget, Kraken, Lighter or Paradex patterns, and no
  insufficient-balance category.

These are fixed in the R5 TS PR.

### Recorded-payload corpus

- **Location:** `sdk-rs/venues/<venue>/tests/fixtures/errors/<case>.json`.
  Each file has these fields:
  - `source`: `live` or `docs`;
  - `captured_at`;
  - `endpoint`, plus `channel` for WS;
  - `http_status`;
  - `body`, verbatim after redaction;
  - `expect`: `{ kind, outcome }` and the path it is pinned on (`write`, `read`, `startup`, `ws_login` or `ws_order`).
- **`live` payloads** are captured from the real venue by the PR3 script.
  Redaction removes keys, signatures, account, order and user IDs, and
  timestamps that identify an account. A redaction test re-checks every file.
- **`docs` payloads** need a doc URL and are allowed only for rate limit, 5xx
  and maintenance (R4).
- **No synthetic payloads.** Existing inline tests stay but are not counted as
  corpus.
- **Tests.** Each fixture is driven through the venue's production funnel
  (write fact, `classify::read`, startup, or WS ingress), not through the table
  alone. The test asserts the kind, the outcome and the visible message. A
  shared loader in `safe_cex_engine::test_support` parses each file and checks
  its provenance and redaction.

### PR sequence (all target master; each needs claude-8x5tm1's exact-tree GO)

1. **PR1 (this section).** It supersedes #2237. Caller fragment:
   `docs/plans/callers/rust-error-classification-beta.md`.
2. **PR2, engine and bridge.**
   - `ErrorKind::InsufficientBalance` (wire `insufficientBalance`) and its
     `as_str` arm, plus the serde variant-list test.
   - `host.mjs` `ERROR_KINDS` and the `host.d.mts` union. Today the bridge
     silently drops unknown kinds, so PR2 lands before any family emits the new
     kind.
   - `commandOutcome.ts` union, `KINDS` and `CATEGORIES`; a new
     `insufficient_balance` category in `errorPatterns.ts`;
     `ExchangeHealthMonitor` `SPECIFIC_KINDS`.
   - The corpus loader.
   - No venue behaviour change.
3. **R5 TS PR:** the `errorPatterns.ts` fixes listed above, with tests.
4. **PR3:** the local-only capture script with redaction and its test. It never
   runs in CI and the repo holds no keys.
5. **One family per PR.**
   - Order: Binance, Bybit, OKX, Bitget, Hyperliquid, Kraken (futures, plus a
     new spot table), Lighter, Paradex (new table), Bitunix, Phemex, BingX, ApeX,
     BloFin, Crypto.com, AsterDEX.
   - Each PR:
     - re-points table rows: balance to `InsufficientBalance`, order rules to
       `InvalidInput`, and the `unlisted` verdict under R3;
     - deletes that family's ladders, copies and text matches above;
     - routes reads, startup, WS login and WS order rejects through the table;
     - adds corpus fixtures with RED-first production-binding tests;
     - lists the kind deltas, R3 flips and the TS-wrong list in its caller
       fragment.
   - The venue owner reviews. Before starting, the family is checked for open
     PRs on the same files.
   - The Hyperliquid PR changes classification only, never freshness.

Out of scope: non-beta families, Retry-After and weight parsing (rate-limit
core), and order-store reject semantics (R2).

## Inventory (ranked by production impact)

Paths are relative to `sdk-rs/venues/<venue>/src/`. "None" means
`outcome: None` (unclassified). Every row was found by per-family sweeps of
`ErrorKind::`, `Outcome::`, status comparisons, native-code matches, `contains`/
`starts_with` and `partial` usage, and was re-read in context. The P0 Bybit
batch rows were re-verified by hand. Re-verify each row against current source
before fixing it, because other lanes may have landed it already.

**Re-verified at `70da1f50e87` (read-only).** Struck rows are fixed at the cited PR.
Multi-venue rows stay unstruck while any venue in them is open; their per-venue
status is below.

- Row 16: bybit fixed (#1914), bitget fixed (#2122).
- Row 17: hyperliquid and paradex open.
- Row 18: kraken spot open; breakoutprop and txflow open but deprioritized.
- Row 21: bitrue fixed (#2201); grvt, lighter, woo, kraken futures and coinbase open.
- Row 34: weex fixed (#1934); okx, bingx and bitget open (the write auth table
  misses the read side's auth codes); kucoin open but deprioritized.
- Row 35: apexomni, bitrue, cryptocom, blofin, bingx and backpack fixed (their
  tables declare `unlisted`); kraken, woo, mexc and polymarket open (no
  CodeTable).
- Row 36: binance fixed for writes (#1837); asterdex partly fixed (#2166: code 0
  is filtered, but the status table is empty); bybit open.
- Row 37: backpack fixed (#1946); kraken futures, breakoutprop and coinbase open.
- Expected to close at landing, to be re-checked then: lighter rows 10, 21 and
  23 and the 21104 resend (#2230); orderly row 24 (Orderly O2).

### P0: false success, or resending a possibly executed write

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| ~~1~~ | ~~cryptocom~~ | ~~`trading.rs:393-405`~~ | ~~`best_effort` PlaceOrders/CancelOrders reach `done` even when every leg failed. Cancel never records IDs, so CancelOrders always returns `done("[]")`.~~ Fixed #2088. |
| ~~2~~ | ~~backpack~~ | ~~`trading.rs:414,938-941`~~ | ~~Every CancelOrders leg is `optional`, so a command where every cancel failed ends `done("[]")`.~~ Fixed #1946 (superseded: managed Plans, done_plan fails on any failed leg). |
| ~~3~~ | ~~bybit~~ | ~~`trading.rs:1028-1060`~~ | ~~A 2xx retCode-0 PlaceBatch whose body fails to decode, or has no list, reports success with zero IDs. Leg count is never checked.~~ Fixed #1914. |
| ~~4~~ | ~~bybit~~ | ~~`trading.rs:1089-1110`~~ | ~~A cancel-batch whose `retExtInfo.list` is missing or short marks every unmatched requested ID cancelled.~~ Fixed #1914. |
| ~~5~~ | ~~apexomni~~ | ~~`lib.rs:165-178`~~ | ~~Resends a write when the message contains `"sign"`, at any status, including 5xx.~~ Fixed #1891 (writes; the "sign" text match is reads-only now). |
| 6 | grvt | `lib.rs:918` | Code 1000 re-login and resend, with no status <500 check. |
| ~~7~~ | ~~bybit~~ | ~~`lib.rs:675-698,2396`~~ | ~~Text-matched (`recv_window`, `error sign`) re-sign and resend of writes, including from transport-failure text, outside `RetryPolicy`.~~ Fixed #1914. |
| ~~8~~ | ~~bitrue~~ | ~~`lib.rs:829-838,905-915`~~ | ~~−1022 re-sign and resend at any status.~~ Fixed #2201. |
| ~~8a~~ | ~~cryptocom~~ | ~~`trading.rs:418-434` (5xx body reaches it through `lib.rs:636`)~~ | ~~40101 re-sign and resend at any status, including 5xx.~~ Fixed #2088. |
| ~~8b~~ | ~~asterdex~~ | ~~`request.rs:262-285`~~ | ~~−1022/−1021 re-sign or clock resend at any status. The code is compared as `f64` (`== Some(-1022.0)`), with message-text fallback.~~ Fixed #2166. |
| ~~9~~ | ~~cryptocom~~ | ~~`lib.rs:636-641`~~ | ~~A native code on 408/5xx is classified by the table, so a 500 carrying 306 becomes `rejected`.~~ Fixed #2088. |
| 10 | grvt, lighter | `wire.rs:202`; `trading.rs:776-829` | A 408 carrying a native code becomes `rejected`. |

### P0: accepted IDs or uncertainty lost on a failing multi-leg command

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| ~~11~~ | ~~okx~~ | ~~`trading.rs:1001-1023`~~ | ~~`refused()` erases explicit Unknown for 50001/50013/50026 ACKs to None. Accepted IDs are dropped (no partial).~~ Fixed #2117. |
| ~~12~~ | ~~blofin~~ | ~~`trading.rs:1385-1406,1505`~~ | ~~Accepted row IDs are dropped on failure, and optimistic rows are removed.~~ Fixed #2157. |
| 13 | kucoin | `trading.rs:781`; `spot/trading.rs:228` | Accepted IDs are dropped (futures and spot). |
| ~~14~~ | ~~weex~~ | ~~`trading.rs:1536-1575`~~ | ~~Accepted batch row IDs never reach `job.ids` or a partial, though the orders are already stored.~~ Fixed #1934. |
| ~~15~~ | ~~phemex~~ | ~~`trading.rs:1479-1492`~~ | ~~Accepted IDs are dropped.~~ Fixed #2130. |
| 16 | bybit, bitget | `trading.rs:800-830`; `trading.rs:1710,1749,1519` | No partial. Bitget keeps IDs only as Debug text. |
| 17 | hyperliquid, paradex | `trading.rs:993-1003`; `trading.rs:93-120` | A mixed batch returns before recording accepted oids. |
| 18 | kraken spot, breakoutprop, txflow | `spot/trading.rs:351-362`; `trading.rs:670-681`; `trading.rs:864-877` | Accepted IDs are dropped entirely. |
| ~~19~~ | ~~asterdex~~ | ~~`trading.rs:426`~~ | ~~`continue_on_error` PlaceOrders with any success reaches `done`. Failed legs appear only in `command_error`.~~ Fixed #2166. |
| ~~20~~ | ~~backpack~~ | ~~`trading.rs:771-779`~~ | ~~One failed Pair leg makes both legs Unknown and drops the placed ID.~~ Fixed #1946. |
| 21 | grvt, lighter, woo, kraken futures, bitrue, coinbase | `trading.rs:1393-1405`; `trading.rs:1029-1034`; `trading.rs:1137-1153`; `trading.rs:965-990`; `trading.rs:1029-1050`; `trading.rs:1606-1640` | IDs survive only in message text or hand-written JSON, not structured `partial`. |
| ~~22~~ | ~~bitrue~~ | ~~`wire.rs:61` → `trading.rs:1527`~~ | ~~A malformed 2xx write reply gets None instead of Unknown.~~ Fixed #2201. |
| 23 | lighter | `account.rs:2087`; `limits.rs:224-231` | ChangeTier write failures have no outcome, even after transport loss or a 5xx. |
| 24 | orderly | `trading.rs:1781,1513` | −1007 is left None and skips the uncertain-write account reconcile. |
| 25 | coinbase | `trading.rs:1533-1535` | `DUPLICATE_CLIENT_ORDER_ID` "may name a live order" but is None, not Unknown. |

Kraken beta remediation (rows 18 and 21, Kraken portions only): failures now
retain structured accepted IDs and explicit Unknown after effects, with shared
order-lock reconciliation. Rows 30 and 37 are corrected for Kraken: one Spot
HTTP auth refusal does not deactivate; Futures native codes pass through the
CodeTable on non-2xx. Other venues listed in those inventory rows remain separate.

### P1: wrong kind, outcome or auth decision in the safe direction

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| 26 | asterdex | `request.rs:217` | A signer failure (including during −1021/−1022 resends) becomes Unknown instead of NotSent. Isolated 2026-10-26: a first-attempt signer refusal is NotSent; only a −1022 refusal followed by a granted resend refused before release is Unknown, by the engine journal contract. Kept for beta (overseer ruling); see the post-beta follow-up under Progress. |
| 27 | phemex | `lib.rs:624-626,354` | Every write status ≥400 becomes Unknown with no kind (auth and rate-limit kinds are lost). |
| 28 | txflow | `lib.rs:889-891` → `trading.rs:772` | A write 4xx becomes `network`/Unknown, and the native body is discarded. |
| 29 | orderly | `account.rs:1096-1105` | Any 4xx on the Info read permanently deactivates the account. |
| 30 | kraken spot | `spot/account.rs:669` | Any 401/403 on any private request deactivates the venue. |
| ~~31~~ | ~~breakoutprop~~ | ~~`lib.rs:816`~~ | ~~Login 401/403 carries no kind, so it is emitted as `REQUEST_FAILED`, not `AUTH_REJECTED`.~~ Fixed #1908. |
| 32 | paradex | `account.rs:510-521` | Auth refusals are retried every 2 s forever, with no auth kind. |
| 33 | bitunix | `recovery.rs:209-216` | Read 401/403/404 → `network`. 10007 is `authInvalid` on writes but `invalidInput` on reads. **Fixed:** account reads take their kind from the shared `BITUNIX` table (status rows, read-scoped 10007 → `authInvalid`); `tests/account_read_classification.rs`. |
| 34 | okx, bingx, bitget, kucoin, weex | `trading.rs:1072-1078` vs `account.rs:2190-2197`; `account.rs:1729`; `trading.rs:1920-1925`; `wire.rs:302,313`; `trading.rs:376` | Auth tables diverge or are ignored on writes. The OKX write table lacks 50100/50101/50103-50105. KuCoin 400002 (clock skew) arrives as HTTP 401, so the status branch decides `authInvalid` before the table; the fix depends on rule 4 overriding status. Bitget/WEEX native auth codes on writes get kind `rejected`. |
| 35 | apexomni, bitrue, cryptocom, kraken, woo, blofin (`trading.rs:1584`), bingx, kucoin, mexc, backpack, polymarket | Each family's table fallback | An unlisted native code gives None. Each table must declare `unlisted`. |
| 36 | binance, asterdex, bybit | `account.rs:324`; `account.rs:353-357`; `lib.rs:357` | A non-JSON 4xx becomes code 0 / HTTP status in the native-code namespace (Bybit leaks the status as `venue_code`, `refresh.rs:258`). |
| 37 | kraken futures, breakoutprop, coinbase, backpack | `trading.rs:727-740`; `lib.rs:835-843`; `trading.rs:1467`; `trading.rs:906-912` | A parsed native code is ignored on non-2xx, or a 2xx in-body refusal is None. |

### P2: generic copies and text classification to delete on migration

- **Status ladders and constructors** (rules 2–6, delete on adoption): every family. Named helpers: binance `trading.rs:1847,1870`; asterdex `trading.rs:1046`, `request.rs:91`; apexomni `trading.rs:1012`; backpack `trading.rs:1053,1067`; bitrue `trading.rs:1156,1504`; coinbase `trading.rs:1399-1477`; cryptocom `trading.rs:551`; bingx `trading.rs:225-270`; blofin `trading.rs:1659-1684`; kucoin `wire.rs:273-321`; mexc `trading.rs:1516-1557`; weex `trading.rs:364-395`; phemex `trading.rs:56` (local Outcome enum); bybit `trading.rs:210,960,2496,2514`, `hyro.rs:556-627` (divergent second table); okx `trading.rs:1041-1095`; bitget `trading.rs:1631-1640,1913-1936,2139`; bitunix `error.rs:39-178`; grvt `wire.rs:178-250`; hyperliquid `trading.rs:940-952`, `trading/schedule.rs:87`; lighter `trading.rs:660-850,929,1403`; orderly `trading.rs:1203-1215,1765`; paradex `trading.rs:93,949,1451-1474`; polymarket `trading.rs:856-904`; kraken `trading.rs:993-1116`, `spot/trading.rs:449-494`; woo `trading.rs:1137-1197,1514-1585`; txflow `trading.rs:931-977`; breakoutprop `lib.rs:1246-1262`, `trading.rs:234,247`.
- **Message text becomes codes, or explicit `MessagePrefix` rows with a cited reason:**
  - bybit `trading.rs:226,232`, `lib.rs:396-414`, `hyro.rs:542,611`, `account.rs:2507`
  - bitget `trading.rs:1659`, `account.rs:2660`
  - bitunix `trading.rs:1359,1396`
  - binance `account.rs:1926` (POST_ONLY/GTX/MAKER)
  - asterdex `request.rs:262-278`, `trading.rs:464`
  - bingx `lib.rs:113`, `account.rs:828`
  - blofin `trading.rs:1486,1535`, `account.rs:1123,1132` (raw body substring)
  - mexc `trading.rs:1275`
  - hyperliquid `trading.rs:1238`
  - lighter `account.rs:2008`
  - kraken `spot/account.rs:656-668`, `account.rs:807`
  - woo `trading.rs:1301,1504`
- **Venue parsing of engine- or venue-formatted error strings** (forbidden after
  migration; use the typed failure instead): bitrue `lib.rs:916-926`,
  `account.rs:2003`; bybit `account.rs:272`; grvt `account.rs:1854`; cryptocom
  `account.rs:485`, `history.rs:561`.
- **Out of this lane's scope, handed to the owning cores:**
  - Venue-local Retry-After/limit parsers go to the rate-limit core: binance `lib.rs:485`, bitunix `error.rs:120`, `recovery.rs:187`, weex `rate.rs:3`, mexc `lib.rs:799`, phemex `lib.rs:614`.
  - The public `depth_query` failure helper, duplicated across backpack, cryptocom, grvt, paradex and weex, goes to the read scheduler.
  - WS login ACK decoding goes to the socket-auth core (#1771), which consumes `CodeTable::verdict` for the codes.

## Phases

1. **Plan (this document).** Committed and independently reviewed; API shape
   sent to codex-9czdjh, codex-zs5e83, codex-ih35js and codex-hcmq3f.
   Prerequisite seam #1808 (`request_context::Error::venue_error`).
2. **Engine core.** `classify.rs` with `Verdict`/`CodeTable`/strict decode,
   `assert_table` and `ResendGrant`. The `Cx` write/batch/read failure entries
   plug into the existing `CommandWrites`/`ResponseProof` path.
   `http_write_result` is narrowed to `Accepted` and gains an expected-count check.
   `RetryEvidence::DefinitiveRefusal` becomes grant-only, with a one-shot flag per
   leg. Add `retry_after_ms` and `auth_outcome`. Binance is the only managed
   venue, so its `http_write_result` callers, its signing `not_sent` path and
   its batch projection (`trading.rs:945-985`) move to `http_write_batch` in the
   same PR. The narrowed API therefore never breaks a caller.
   - Pure precedence tests first: rules 1–10, a code on 408/5xx, a non-JSON 4xx
     with and without a status row, `f64`-lookalike codes, code-over-status
     (KuCoin 401+400002), intent-scoped `Satisfied`, a forged or second resend
     refused, resend refused on 5xx, and a batch count mismatch.
   - The documented-4xx survey (rule 5) lands in this phase.
   - Coordinate with codex-uufub4 before touching `http_lifecycle.rs` or
     `request_context*.rs`.
   - **2b:** an engine-owned uncertain-write reconcile reaction, plus the rule 5
     rate-window deferral.
3. **Proof through two venues, each with a failing test before the passing one.**
   - **Binance (managed):** replace `trading.rs:1847 failure`, `account.rs:311
     venue_error` and the special codes with `BINANCE: CodeTable`. This sits on
     the phase 2 branch, which already moved the batch projection.
     RED test: a non-JSON 429/418 on a write currently reports `unknown` with no
     kind (#36). It must report `rejected` + `rateLimited` from Binance's
     documented status rows, with `retryAfterMs` when the header is present.
   - **KuCoin (futures and spot share `wire.rs:280`, with no open PR).**
     - RED tests: 400002 with HTTP 401 is not `authInvalid` (#34), a 418 write
       is `rateLimited`, and a failed multi-order PlaceOrders keeps the accepted
       IDs (#13).
     - #13 only closes through the write journal, so this proof includes KuCoin's
       RequestContext write adoption. Before starting it, confirm with claude-ddi2il
       that no venue lane owns KuCoin; if one does, use that lane's adoption instead.
     - This is the largest PR in the plan and may become its own lane, as
       claude-ddi2il decides.
4. **Fan-out, one family per PR, in P0 order.** Venues with open PRs touching
   their source wait for those PRs or go through the owning lane. Each venue lane
   adopting RequestContext writes must use the table plus rule 9, which closes
   rows #11–#21 for that venue. Strike rows here and in RUST_AUDITS.md as they land.

Each PR answers the architecture gate question in its body ("is this a per-venue
fix of a cross-cutting concern?"). Every fix here deletes a venue copy in favour
of the engine path.

## Callers (tealstreet-next)

No message text changes. Expected `kind`/`outcome` corrections that callers
reading `ExchangeError` fields will see (`opts.classified`, `isUnknownOutcome`,
`exchangeErrorTags`):
- Writes previously None become `unknown` or `rejected`.
- A bare 4xx write without a documented status row flips from `rejected` to `unknown` (with an auth or rate-limit kind). Families with documented rows keep `rejected`. Each PR lists its family's flip.
- 5xx/408 coded writes on Crypto.com, GRVT and Lighter become `unknown`.
- Phemex/TxFlow 4xx gain auth and rate-limit kinds.
- KuCoin 400002 stops being `authInvalid`.
- False-success Crypto.com/Backpack batches become failures carrying partial IDs.

Each PR lists its exact deltas under [RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md)
"Error strings that must move to codes".

## Progress

- 2026-10-23, phase 2a (core plus the Binance proof):
  - `engine/src/classify.rs` provides the precedence, the tables, strict
    decoding, the batch projection, header-only Retry-After and credentialed
    auth evidence.
  - The `Cx` entries `http_write_failure`, `http_write_unproven`,
    `http_write_batch` and `http_resend_context` are added.
  - Resends need an engine `ResendGrant`, one per logical leg, and
    `http_retry_context` refuses write evidence.
  - The Binance `failure()` match becomes the `BINANCE` table with documented
    status rows (#36 closed for Binance).
- Deferred to phase 2b ("write fact authority"):
  - narrowing `http_write_result` to `Accepted`;
  - binding intent and expected count at leg registration;
  - classifying from the engine-delivered response;
  - `http_write_not_sent`;
  - `http_read_failure`/`ReadRetry`;
  - the credentialed `account_auth_outcome` argument;
  - the rule 5 deferral, `reconcile_uncertain` and Satisfied reconcile marking;
  - the cancel-echo check;
  - Binance's move to `http_write_batch`, and its read-side `venue_error`.
- 2026-10-23, phase 2b (write fact authority and reads):
  - Refusals exist only as `classify::Fact`. A fact comes from
    `Cx::classify_write`, which uses the engine-delivered reply and the intent
    captured on the command owner at dispatch, and it is bound to its attempt.
  - `Cx::http_write_fact` records `WriteFact` and refuses an empty placement
    acknowledgement. `http_write_leg_intent` narrows a leg before its first
    attempt.
  - `classify::read` returns `ReadFailure` / `ReadRetry` with no `Outcome`.
  - Auth evidence needs no `credentialed` argument: the engine consults
    `account_auth_outcome` only for credentialed account reads
    (`PendingHttp.auth`), and the hook uses `classify::auth_outcome`.
  - Still deferred to phase 2c: the rule 5 deferral, `reconcile_uncertain`,
    Satisfied reconcile marking, the cancel-echo check, and Binance's
    read-side `venue_error`.
- Phase 2b decisions from venue-lane review (codex-zs5e83):
  - **Exact message rows.** Add `Key::Message`, an exact match on the native
    message. The venue codec strips only its documented suffix grammar while
    decoding (Hyperliquid: trailing punctuation/whitespace or ` asset=<digits>`)
    and rejects anything else. A bare `MessagePrefix` widens success evidence,
    so reconsider removing it.
  - **Intent from the logical command.** The intent is bound at leg
    registration from the logical command, not from the endpoint. The cancel
    leg of an amend/replace is `Intent::Amend`, so a `Satisfied(Cancel)` row
    cannot authorise a replacement.
  - **408 never resends.** 408 is decided before any table lookup, so it can
    never yield `RefusedResend`; Lighter's 21104 nonce resend currently admits it.
- Row #17 correction (Paradex): `on_response` already extends accepted IDs
  (`trading.rs:848-855`). The loss is `Job::failure` (`trading.rs:93-120`)
  dropping the structured partial on a failing multi-leg command, and a mixed
  placement reaching `done` (`trading.rs:793`).
- A batch whose only effects are `Satisfied` items records `KnownPartial`
  without partial counts. The journal already treats that as known effects
  without IDs.
- 2026-10-23, phase 2c (#1857): engine-owned reconcile of uncertain writes,
  plus three items from the Bybit adoption rulings (claude-ddi2il):
  - narrowing Amend→Setting (attached TP/SL legs);
  - a separately declared `http_write_protection_cancel_leg` (Amend→Cancel,
    only for a cancel-then-recreate whose replacement is position-bound);
  - Satisfied(Cancel) reconcile marking: the command's symbols are fenced in
    the order-lock uncertainty map and exactly one `reconcile_uncertain` read is
    queued. It is not a Cancelled transition, and only `orders_reconciled`
    clears the fence.
- Managed logical order reads (prerequisite for Bybit slice 1d): an order-read
  token used to belong to one physical attempt, so a multi-page read whose first
  page was released before a fence cleared it on its last page's delivery. A
  managed Command or Background owner now keeps the epoch of its first released
  Read, and every later leg's delivery reconciles from that mark. A venue whose
  logical read spans pages, coins or products issues them as legs of one owner.
  The mark lives while its owner does (a live context, or a command owner in
  any phase, so a serial Plan keeps it between steps) and is dropped when the
  command or Plan retires. A mark only keeps a fence longer, so a reconcile
  read that must clear one uses a fresh owner, never a leg of a read already
  in flight.
- 2026-10-26, AsterDEX beta (#2270):
  - Non-JSON 4XX write replies classify through `ASTER_STATUS` (400/401/403/418/429), so a gateway 429 or 418 is
    `rateLimited`/Rejected (#36 closed for AsterDEX).
  - #26 isolated, and Unknown kept for beta by overseer ruling. The reconcile read clears it, so it carries no
    money risk.
  - **Post-beta follow-up (engine change, needs 8x5tm1 GO):** a definitive, effects-free refusal followed by a
    retry that never reached transport should project Rejected. `CommandWrites` counts a released attempt as
    uncertain whenever a later attempt on its leg is unreleased, even one with a final NotSent result. Narrowing
    that to a pending later attempt flips exactly four contract tests that pin the current Unknown:
    `earlier_released_retryable_refusal_then_failed_retry_is_unknown_for_every_failure_cause`,
    `released_native_refusal_then_retry_preparation_failure_is_unknown_not_not_sent`,
    `prior_send_evidence_survives_native_refusal_retry_and_signer_failure` and
    `observer_projection_does_not_consume_definitive_refusal_retry`. AsterDEX's
    `a_refused_resend_signature_after_a_signature_rejection_is_unknown` pins the venue-level outcome.
