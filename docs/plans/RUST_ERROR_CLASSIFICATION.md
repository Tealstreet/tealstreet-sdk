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
   `SignError::venue_error()`).
2. **Released, then transport failure → write `unknown` / `network`.** A read gets
   `network`, and read-retry eligibility applies only under `RetryPolicy::ReadFailure`.
3. **408 or ≥500 → write `unknown`** (`timeout` / `venueUnavailable`), whatever
   the body says. A native code in that body is kept only as
   `venueCode`/`venueMessage` evidence and never makes the write `rejected`.
4. **A decoded native error code → venue table verdict.** Integer codes decode
   strictly (exact integer or canonical quoted integer, never `f64`). String codes
   compare exactly.
5. **Non-2xx with no decodable native error → `unknown`, with a status-derived
   kind** (401 `authInvalid`, 403 `authPermission`, 418/429 `rateLimited`).
   "HTTP status alone is not refusal evidence." A venue may declare a status row
   only where its documentation proves the request was refused before execution,
   with that citation next to the row.
6. **2xx whose success envelope or ACK is malformed, incomplete or count-mismatched →
   write `unknown` / `internal`.** For a read, `internal` and a malformed failure.
7. **`satisfied` verdict → success for its declared intent only** (already
   cancelled, not modified, already set). It is never a whole-command success for
   another intent.
8. **One-shot re-sign resend** only on a `refusedResend` verdict, under rules 3–4
   (status <500 and ≠408), granted as `RetryEvidence::DefinitiveRefusal` through
   `Context::retry`. It never comes from message text or engine-formatted strings.
9. **Aggregation stays in `CommandWrites::fail`**, which already holds the
   `notSent`/`rejected`/`unknown`/partial projections and the accepted-ID
   join. Venues record a per-leg fact; they never pick the command outcome or
   demote it after an effect.

`kind` comes from the verdict. A `rateLimited` kind gets `retryAfterMs` from the
engine's existing Retry-After parser (`startup::rate_limit_pause_ms`).
Venue-specific weight/limit headers belong to the rate-limit core (audit rank 10),
not to this lane. Caller-visible text is unchanged: `message` keeps the venue's
current text, and the native code and message go in `venueCode`/`venueMessage`.

## Code-table API (published to venue lanes before implementation)

Module `engine/src/classify.rs`, re-exported as `safe_cex_engine::classify`.
Tables are `const` venue data and contain no logic.

```rust
pub enum Verdict {
    Refused(ErrorKind),        // proven pre-execution refusal: write `rejected`
    RefusedResend(ErrorKind),  // proven refusal that permits one re-signed resend
    Unknown(Option<ErrorKind>),// code does not prove the outcome: write `unknown`
    Satisfied(Intent),         // requested state already holds, for this intent only
}
pub enum Intent { Place, Cancel, Amend, Setting, Read }
pub enum Key { Int(i64), Range(i64, i64), Text(&'static str), MessagePrefix(&'static str) }
pub struct Row { pub key: Key, pub verdict: Verdict, pub only: Option<Intent> }
pub struct CodeTable {
    pub rows: &'static [Row],
    pub unlisted: Verdict,             // required; documented default for unlisted codes
    pub status: &'static [(u16, Verdict)], // documented pre-execution statuses only (rule 5)
}
pub enum NativeCode<'a> { Int(i64), Text(&'a str) }
pub struct Native<'a> { pub code: NativeCode<'a>, pub message: &'a str }

impl CodeTable {
    pub const fn new(rows: &'static [Row], unlisted: Verdict) -> Self;
    pub fn verdict(&self, code: NativeCode<'_>, intent: Intent) -> Verdict; // WS login, startup
}
pub fn decode_int(token: &str) -> Option<i64>; // strict integer, rule 4

// Managed path (Cx), recording the fact in the command journal:
cx.http_write_failure(&context, &TABLE, intent, &response, native) -> Verdict;
cx.http_write_unproven(&context, message) ;          // rule 6
cx.http_read_failure(&context, &TABLE, &response, native) -> ReadFailure;
pub struct ReadFailure { pub error: VenueError, pub retry: Option<RetryEvidence>,
                         pub auth: Option<AuthOutcome> }
```

- **`MessagePrefix`** applies only to the venue's own native message field, and
  only for venues with no native codes (Hyperliquid, TxFlow, Polymarket). It is an
  exact prefix, never `contains`. Each row cites the doc or tape that proves the
  text is stable.
- **One table per venue family** serves writes, reads, startup and WS login. A
  `Row.only` restricts a verdict to one intent, for example Binance −2011 is
  `Satisfied(Cancel)` only.
- **Table tests:** a shared `classify::assert_table(&TABLE)` rejects overlapping
  ranges, duplicate keys and a `Satisfied(intent)` row whose `only` disagrees.
  Each venue table calls it from its own test.
- **Legacy, unmanaged venues** call the same pure `classify::write(&TABLE, intent,
  status, native)` and get a `VenueError` back. This is one engine function, not a
  fallback path. Partial-ID preservation and aggregation become correct only when
  a venue adopts the RequestContext write journal (rule 9). The venue lanes
  consume this API in their adoption PRs.

The API shape may tighten during phase 2 review. Changes are announced to the
venue lanes before merge.

## Inventory (ranked by production impact)

Paths are relative to `sdk-rs/venues/<venue>/src/`. "None" means
`outcome: None` (unclassified). Every row was found by per-family sweeps of
`ErrorKind::`, `Outcome::`, status comparisons, native-code matches, `contains`/
`starts_with` and `partial` usage, and was re-read in context. The P0 Bybit
batch rows were re-verified by hand. Re-verify each row against current source
before fixing it, because other lanes may have landed it already.

### P0: false success, or resending a possibly executed write

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| 1 | cryptocom | `trading.rs:393-405` | `best_effort` PlaceOrders/CancelOrders reach `done` even when every leg failed. Cancel never records IDs, so CancelOrders always returns `done("[]")`. |
| 2 | backpack | `trading.rs:414,938-941` | Every CancelOrders leg is `optional`, so a command where every cancel failed ends `done("[]")`. |
| 3 | bybit | `trading.rs:1028-1060` | A 2xx retCode-0 PlaceBatch whose body fails to decode, or has no list, reports success with zero IDs. Leg count is never checked. |
| 4 | bybit | `trading.rs:1089-1110` | A cancel-batch whose `retExtInfo.list` is missing or short marks every unmatched requested ID cancelled. |
| 5 | apexomni | `lib.rs:165-178` | Resends a write when the message contains `"sign"`, at any status, including 5xx. |
| 6 | grvt | `lib.rs:918` | Code 1000 re-login and resend, with no status <500 check. |
| 7 | bybit | `lib.rs:675-698,2396` | Text-matched (`recv_window`, `error sign`) re-sign and resend of writes, including from transport-failure text, outside `RetryPolicy`. |
| 8 | bitrue | `lib.rs:829-838,905-915` | −1022 re-sign and resend at any status. |
| 9 | cryptocom | `lib.rs:636-641` | A native code on 408/5xx is classified by the table, so a 500 carrying 306 becomes `rejected`. |
| 10 | grvt, lighter | `wire.rs:202`; `trading.rs:776-829` | A 408 carrying a native code becomes `rejected`. |

### P0: accepted IDs or uncertainty lost on a failing multi-leg command

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| 11 | okx | `trading.rs:1001-1023` | `refused()` erases explicit Unknown for 50001/50013/50026 ACKs to None. Accepted IDs are dropped (no partial). |
| 12 | blofin | `trading.rs:1385-1406,1505` | Accepted row IDs are dropped on failure, and optimistic rows are removed. |
| 13 | kucoin | `trading.rs:781`; `spot/trading.rs:228` | Accepted IDs are dropped (futures and spot). |
| 14 | weex | `trading.rs:1536-1575` | Accepted batch row IDs never reach `job.ids` or a partial, though the orders are already stored. |
| 15 | phemex | `trading.rs:1479-1492` | Accepted IDs are dropped. |
| 16 | bybit, bitget | `trading.rs:800-830`; `trading.rs:1710,1749,1519` | No partial. Bitget keeps IDs only as Debug text. |
| 17 | hyperliquid, paradex | `trading.rs:993-1003`; `trading.rs:93-120` | A mixed batch returns before recording accepted oids. |
| 18 | kraken spot, breakoutprop, txflow | `spot/trading.rs:351-362`; `trading.rs:670-681`; `trading.rs:864-877` | Accepted IDs are dropped entirely. |
| 19 | asterdex | `trading.rs:426` | `continue_on_error` PlaceOrders with any success reaches `done`. Failed legs appear only in `command_error`. |
| 20 | backpack | `trading.rs:771-779` | One failed Pair leg makes both legs Unknown and drops the placed ID. |
| 21 | grvt, lighter, woo, kraken futures, bitrue, coinbase | `trading.rs:1393-1405`; `trading.rs:1029-1034`; `trading.rs:1137-1153`; `trading.rs:965-990`; `trading.rs:1029-1050`; `trading.rs:1606-1640` | IDs survive only in message text or hand-written JSON, not structured `partial`. |
| 22 | bitrue | `wire.rs:61` → `trading.rs:1524` | A malformed 2xx write reply gets None instead of Unknown. |
| 23 | lighter | `account.rs:2087`; `limits.rs:224-231` | ChangeTier write failures have no outcome, even after transport loss or a 5xx. |
| 24 | orderly | `trading.rs:1781,1513` | −1007 is left None and skips the uncertain-write account reconcile. |
| 25 | coinbase | `trading.rs:1533-1535` | `DUPLICATE_CLIENT_ORDER_ID` "may name a live order" but is None, not Unknown. |

### P1: wrong kind, outcome or auth decision in the safe direction

| # | Venue | Site | Defect |
| --- | --- | --- | --- |
| 26 | asterdex | `request.rs:217` | A signer failure (including during −1021/−1022 resends) becomes Unknown instead of NotSent. |
| 27 | phemex | `lib.rs:624-626,354` | Every write status ≥400 becomes Unknown with no kind (auth and rate-limit kinds are lost). |
| 28 | txflow | `lib.rs:889-891` → `trading.rs:772` | A write 4xx becomes `network`/Unknown, and the native body is discarded. |
| 29 | orderly | `account.rs:1096-1105` | Any 4xx on the Info read permanently deactivates the account. |
| 30 | kraken spot | `spot/account.rs:669` | Any 401/403 on any private request deactivates the venue. |
| 31 | breakoutprop | `lib.rs:816` | Login 401/403 carries no kind, so it is emitted as `REQUEST_FAILED`, not `AUTH_REJECTED`. |
| 32 | paradex | `account.rs:510-521` | Auth refusals are retried every 2 s forever, with no auth kind. |
| 33 | bitunix | `recovery.rs:209-216` | Read 401/403/404 → `network`. 10007 is `authInvalid` on writes but `invalidInput` on reads. |
| 34 | okx, bingx, blofin, bitget, kucoin, weex | `trading.rs:1077` vs `account.rs:2193`; `account.rs:1729`; `trading.rs:1584`; `trading.rs:1920-1925`; `wire.rs:313`; `trading.rs:376` | Auth tables diverge or are ignored on writes. KuCoin 400002 (clock skew) is `authInvalid`. Bitget/WEEX native auth codes on writes get kind `rejected`. |
| 35 | apexomni, bitrue, cryptocom, kraken, woo, blofin, bingx, kucoin, mexc, backpack, polymarket | Each family's table fallback | An unlisted native code gives None. Each table must declare `unlisted`. |
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
2. **Engine core.** `classify.rs` with `Verdict`/`CodeTable`/strict decode, and
   `Cx::http_write_failure`/`http_write_unproven`/`http_read_failure` wired into
   the existing `CommandWrites`/`ResponseProof` path. Add `assert_table` and pure
   precedence tests: rules 1–8, a code on 408/5xx, a non-JSON 4xx, `f64`-lookalike
   codes, intent-scoped `Satisfied`, and resend refused on 5xx. Coordinate with
   codex-uufub4 before touching `http_lifecycle.rs`/`request_context*.rs`.
3. **Proof through two venues, each with a failing test before the passing one.**
   - **Binance (managed):** replace `trading.rs:1847 failure`, `account.rs:311
     venue_error` and the special codes with `BINANCE: CodeTable`. RED test: a
     non-JSON 429/418 on a write currently reports Unknown with no kind (#36). It
     must report Unknown with kind `rateLimited` and `retryAfterMs`.
   - **KuCoin (legacy, futures and spot share `wire.rs:280`, with no open PR):**
     replace `write_failure` with `KUCOIN: CodeTable` through `classify::write`.
     RED tests: 400002 is not `authInvalid` (#34), and a 418 write is
     `rateLimited` Unknown, not plain Rejected.
   - #13 (partial IDs) needs KuCoin's RequestContext write adoption. It is listed
     as an adoption requirement, not patched locally.
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
- 5xx/408 coded writes on Crypto.com, GRVT and Lighter become `unknown`.
- Phemex/TxFlow 4xx gain auth and rate-limit kinds.
- KuCoin 400002 stops being `authInvalid`.
- False-success Crypto.com/Backpack batches become failures carrying partial IDs.

Each PR lists its exact deltas under [RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md)
"Error strings that must move to codes".
