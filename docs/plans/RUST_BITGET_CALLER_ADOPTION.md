# Bitget caller adoption, slice 1: candles, identity, account reads, trade entry

Plan, 2026-10-24. Branch `feat/rust-bitget-caller-entry` starts from Bitget
signer PR #1915 head `a56e6fa4f1c`. #1915 is stacked on the OKX signer PR #1773.
The #1848 history bridge (`6872554ac85`) is already in this base. This document
holds the plan and the original RED/control evidence; the slice is implemented
in `13edac8bd9a` (review follow-ups after it).

Precedents: OKX slice 1 (#1938, `RUST_OKX_CALLER_ADOPTION.md`) for managed
private reads, Background reads and trade entry refusals, and OKX slice 2
(#1942, `RUST_OKX_CALLER_CANDLES.md`) for managed candle reads. The terminal-only
Mutation refusal follows #1921. PR #1890 (UTA routing, egress refusal walk) owns
the private socket login and routing code. This slice does not touch
`connect`, `private_open`, `private_subscribe` or socket supervision.

## Inventory on this branch

The inventory was recomputed by importing `sdk-rs/scripts/check-request-context.py`
(`g.sites()`, `g.fingerprints()`) and replaying `tokens` / `receiver_start` /
`group_end` with token offsets, which maps each occurrence to its line and
enclosing `fn`. The checker passes and the manifest equals the scan: Bitget has
**32 occurrences in 30 path/hash rows**:

| File | Occurrences | Rows |
|---|---|---|
| account.rs | 1 | 1 |
| history.rs | 12 | 11 |
| identity.rs | 2 | 2 |
| lib.rs | 8 | 8 |
| trading.rs | 9 | 8 |

The landing-pinned assignment rows are stale for `lib.rs`. #1915 moved every
private send, sign and sign failure into `BitgetFutures::send`
(`a921be85`, `5b441164`, `6c77d608`) and `on_sign_error` (`c873db6f`). Those
four rows are the owner-approved `STAGING_BOOTSTRAPS` additions (lib.rs 4 → 8).

Six occurrences are lexical only: a local named `request` inside `matches!`.
They are account.rs:828 `82526dea`, and in trading.rs `trade_response`:
1512 `3e008d0d`, 1647 `9cb2980b`, 1762 `2aa595c9`, and 1923/1936 `ff399ff5`.
They carry no provenance (see Q4).

Bitget has no `FetchLeverage` handler and no leverage-info GET. The "settings
read" candidate is the startup settings probe (an account-cycle read). The
leverage/margin metadata GETs (`Effect::Metadata`, `ResolveAmend`) belong to the
write job.

## In scope (6 occurrences removed)

| Fingerprint | File:line | Function | Class | Replacement |
|---|---|---|---|---|
| `7280fdcd` | lib.rs:1094 | `on_command` FetchCandles | entry refusal (unsupported interval) | admit, then `fail_context(&root, value.unsupported("Bitget"))` |
| `43cf9107` | lib.rs:879 | `on_response` Candles | managed public read, success | `validate_read_http_delivery(&root, &root)`, then `done_context(&root, rows)` |
| `569abc44` | lib.rs:969 | `on_request_failed` Candles | managed read, post-send failure | `fail_context(&root, error)` with unchanged text |
| `c5a021da` | identity.rs:83 | `finish` | managed signed read, success | `validate_read_http_delivery`, then `done_context` |
| `c873db6f` | identity.rs:86 | `finish` | managed signed read, failure | `command_error_context(&root, &text)`, then `fail_context` |
| `2f0770d2` | trading.rs:271 | `trade_command` | entry refusal (no credentials, preflight) | terminal-only Mutation admission, then `fail_context(not_sent(..))` |

Converted with **zero inventory delta**: the account-cycle reads. These are the
settings probe, positions (and the inverse per-coin chain), Classic/UTA balance,
and the open/plan/strategy order pages. All of them go through
`account_request` → `send`. They become Background Read Contexts, which empties
`send` of every account caller (Q1).

Touched but kept:

- **`c873db6f` lib.rs:941 `on_sign_error`.** Identity and Candles get their own
  Context arms. History keeps `let command = request.command; cx.fail(command,
  error)`. This is the same token stream and the same hash, so the row stays at 1
  until the history slice.
- **`a5c4555a` lib.rs:229 `get`.** Candles stop calling it. It still serves
  public discovery (contracts, tickers) under `PublicRetries` (Q5).
- **`send` funnel** `a921be85` (streamed), `5b441164` (body), `6c77d608`
  (`sign_http`). After this slice its only callers are history (streamed,
  signed and unsigned) and the trade job (body, signed). The
  `primary_balance_read` → `route_request(Primary)` branch inside `send`
  becomes provably dead and is deleted in the same change.

## Expected exact delta

- lib.rs goes from 8 to 5 occurrences: `{5b441164, 6c77d608, a5c4555a,
  a921be85, c873db6f}`.
- identity.rs goes from 2 to 0, and the path leaves the manifest.
- trading.rs goes from 9 to 8 (`2f0770d2` goes).
- account.rs and history.rs are unchanged.
- Bitget goes from 32 to 26 occurrences, and from 30 to 24 rows.
- The manifest prune is exactly: remove `43cf9107`, `569abc44` and `7280fdcd`
  from lib.rs, remove identity.rs, and remove `2f0770d2` from trading.rs.

If the owner accepts the lexical renames (Q4), the six `matches!` rows also go:
account.rs leaves the manifest, trading.rs goes to 3, and Bitget goes to 20/19.

## `STAGING_BOOTSTRAPS` (`venues/bitget/src/lib.rs`)

The entry maps the pre-#1915 landing map `{43cf9107, 569abc44, 7280fdcd,
a5c4555a}` to the staged map. The staged map adds `5b441164`, `6c77d608`,
`a921be85` and `c873db6f`.

- After this slice, lib.rs is `{5b441164, 6c77d608, a5c4555a, a921be85,
  c873db6f}`. That is a subset of the staged map, so the checker passes with
  no edit.
- **No funnel site becomes prunable in this slice.** History still needs
  `request_streamed` (`a921be85`) and `sign_http` (`6c77d608`). The trade job
  still needs `request` (`5b441164`) and `sign_http`. History sign failures
  still need `c873db6f`.
- The funnel's exits are the history slice (`a921be85`, `c873db6f`, and
  `6c77d608` for reads) and the write slice (`5b441164`, `6c77d608`). Both
  need history and Plan/write-group authority. The three lib.rs rows this slice
  removes are all landing rows, so the entry stays required while any compared
  base holds the pre-#1915 map. Narrowing the staged map is optional tightening
  that belongs to #1915's owner.

## Native behaviour to preserve (pinned by the controls)

- **Candles.** These are public and unsigned:
  - `GET https://api.bitget.com/api/v2/mix/market/candles?symbol=..&granularity=..&endTime=..&productType=..&limit=..[&startTime=..]`.
  - The header is exactly `X-CHANNEL-API-CODE: Tealstreet`, with no `ACCESS-*`,
    no body and no route.
  - Limit defaults to 500, clamped to 1–1000. `endTime` is clipped to
    `from + min(limit bars, 90 d)`.
  - Failures are hard `CommandFailed` with kind, outcome and diagnostic all
    absent:
    - an unsupported interval gives `Bitget does not support 8h candles`, sent
      before any HTTP;
    - a non-200 gives `Bitget HTTP <status>: <body>`;
    - an envelope code gives `Bitget <code>: <msg>`;
    - a bad row gives `invalid Bitget REST record`;
    - a missing list gives `Bitget successful response has no expected list`;
    - a transport failure gives the transport text.
  - Today there is no engine deadline and no retry.
- **Identity** (`FetchAccount`).
  - `GET /api/v2/spot/account/info` for every product.
  - It signs with the read key when one is configured and otherwise with the
    primary key. Route is `None`. Broker and Content-Type headers come from the
    recipe.
  - Every failure emits one `CommandError` with the same text before
    `CommandFailed`. A signing refusal fails without a diagnostic (today's
    `on_sign_error` arm).
  - Public-only accounts refuse with `Bitget account identity requires
    credentials` before any HTTP. Invalid read credentials refuse with the
    validation text.
- **Account reads.**
  - Exact Classic and UTA URLs, including cursor parameters, inverse per-coin
    positions, and Classic profit_loss/track_plan versus UTA tpsl/trailing_stop
    strategy pages.
  - Classic versus UTA selection comes from the probe (40084 means Classic, a
    successful settings read means UTA, and a later 40085 promotes the account
    and bumps `generation`).
  - The read key is used on every GET. **Exception:** while `degraded`, the
    balance read (`BALANCE` / `UTA_ASSETS`) uses the primary key and primary
    egress. Today that is `RequestRoute::Primary`; managed, it becomes
    `Credential { slot: Primary }`, which gives `RoutingIntent::Primary`. Hosts
    map both to `plan.http` (`hosts/native/src/routing.rs::managed_http`).
  - Generation fencing drops older responses.
  - Probe backoff (`startup::Backoff`), the 40009 two-in-a-row rule, the
    418/429 `rest_paused_until` pause, the 2 s poll cadence (positions every
    tick, balance every 2nd, orders every 4th), the write-check degraded
    recovery and `retried_failure("bitget.account:870")` are all unchanged.
  - A probe transport failure means Classic detection unless UTA is already
    confirmed. A probe provider refusal is not transport evidence.
- **Trade entry.** Refusals keep their text with kind `None` and
  `Outcome::NotSent`, and send no HTTP. A later same-symbol placement still
  writes, so no lock or queued job is left behind.

## Replacement (landed APIs only)

Every API below is on this branch: `admit_http_command`,
`http_context(Scope::Background, ..)`, `request_managed_with_headers`,
`request_signed_managed`, `stream_response`, `validate_read_http_delivery`,
`done_context`, `fail_context`, `command_error_context` and
`finish_http_context`, plus the `on_http_failed` and
`on_http_preparation_failed` hooks. No `http-plans`, Plan, Preflight,
execution group or new engine API is needed. **No shared API is missing.**

1. **Policy.** All reads use one `read_policy(credential)`: `Operation::Read`,
   `timeout_ms: 20_000`, `pacing: None`, `RetryPolicy::Never` (the OKX shape).
   Credential selection reuses `signing_identity`/`primary_balance_read`:
   `Secondary` when a read key exists and the path is not a degraded balance
   read, otherwise `Primary`.
2. **Candles.**
   - Admit first with `credential: None`. On `Err`, return; the engine already
     failed the command.
   - Send with `request_managed_with_headers(root, Get, url, CHANNEL, None,
     pending)`, where `CHANNEL = &[("X-CHANNEL-API-CODE", "Tealstreet")]`.
     `get` keeps its own literal, because changing its tokens would rename the
     discovery fingerprint `a5c4555a`. Then `stream_response(id, Envelope(&[&["data"]]))`.
   - `Pending::Candles` carries `root: Context` instead of `command`.
   - Success is gated by `validate_read_http_delivery`.
   - An override of `on_http_failed` sends the Candles arm to
     `fail_context(&root, error)`, keeping the engine's typed Timeout (Q2).
     Every other failure keeps today's text through `fail_context`.
   - The `on_sign_error` Candles arm, reached through
     `on_http_preparation_failed`, calls `fail_context`.
3. **Identity.**
   - Compute the refusal first (borrowing `&self` only; no IDs are allocated).
     Then admit with credential `None` for a refusal, or the selected credential
     for a send.
   - `request_signed_managed(root, Get, url, None, sign::Http::new(identity,
     PATH), Pending::Identity(root))`.
   - `finish` takes `&Context`:
     - success: validate, then `done_context`;
     - failure: `command_error_context` then `fail_context`.
   - `on_http_failed` passes the `VenueError`, so Timeout keeps its kind, and
     emits the same diagnostic.
   - The `on_sign_error` Identity arm calls `fail_context` with no diagnostic,
     as today.
4. **Account reads.**
   - `account_request` issues `http_context(Scope::Background,
     read_policy(credential))`. The `Pending::Account` variant gains
     `context`. The send is `request_signed_managed` with the same recipe and
     path, plus `stream_response` when `at` is `Some`.
   - Each physical read (a page, the next inverse coin or a promoted probe)
     gets its own Background Context.
   - Issuance or queue refusal is a pre-send failure and goes through the
     existing `account_sign_failed`. For the probe this means `probe_failed`
     with backoff; it is not Classic evidence.
   - Every terminal path calls `finish_http_context(&context)` exactly once
     before the existing logic: response, stream error/malformed, HTTP
     failure, preparation failure, **and stale-generation drops**. Otherwise a
     promoted or rejected generation would leak registry capacity.
   - The `on_http_failed` Account arm keeps the transport route: probe →
     `probe_transport_failed`, other reads → `account_failed`. An engine
     Timeout therefore releases the in-flight latch, and the next poll re-reads.
   - The dead `route_request(Primary)` branch in `send` is deleted.
   - Without `http-plans` the engine does not auto-seal plain Contexts. Every
     path above must therefore finish or complete its Context, and there is no
     numeric fallback.
5. **Trade entry.** Add an OKX-shaped `refuse_entry`.
   - Admit a terminal-only `Operation::Mutation` Context (primary credential
     or `None`, 20 s, `RetryPolicy::Never`), then `fail_context(&context,
     not_sent(error))`. The engine caps it at NotSent.
   - Accepted commands still queue with no admission. The entry loan stays
     Fresh, so the write job's numeric completion stays legal until the write
     slice.
6. **Harness.** `tests/support::Harness::collect` already releases through
   `test_support::prepared_actions`; this was done on this branch and is a
   pass-through for legacy actions. `identity.rs` and other tests that drain
   `engine.drain_actions()` directly switch to `prepared_actions`.

Behaviour changes for callers:

- Candle and identity reads now end at a shared 20 s deadline with a typed
  `Timeout`.
- More than 256 outstanding managed reads get one typed `Capacity` failure
  before any HTTP.
- An unanswered account read is retired at 20 s. Today it latches the poll
  forever. An unanswered probe now takes the existing transport-failure route.
- Managed reads emit `ManagedHttp` preparation. Candles prepare with
  `RoutingIntent::Public`. The degraded balance read prepares with
  `RoutingIntent::Primary`.
- Late replies after a terminal are dropped by the engine.
- Texts, kinds, outcomes and URLs are otherwise unchanged.

Architecture gate: **No.** This adopts the shared entry, managed-HTTP,
Background and Context-completion choke points. It adds no venue-local retry,
timer, ledger, clock or admission. Signing reuses #1915's
`sign::dispatch::Http` unchanged, so `check-signing.py` is unaffected.

## Out of scope

- **history.rs, all 12.** See the next section.
- **trading.rs, 8.**
  - 449 `d4eadb4e` `resume_trading` done and 2177 `5f67f7ee` `trade_fail`
    belong to the ordered write job. They need the Plan/write-group admission
    and reconciliation codecs.
  - 2239 `29da08f8` `trade_timer` fails *queued* commands after detection times
    out. That happens outside the command callback, so `admit_http_command`
    cannot bind it. It needs queue or Plan admission.
  - The five lexical `matches!` rows (Q4).
- **lib.rs, 5.** These are the funnel (above), `a5c4555a` discovery `get`
  (Q5) and the History `on_sign_error` arm.
- **account.rs 828 `82526dea`.** Lexical (Q4).
- **The private socket** (#1890). It has no inventory rows and is untouched.

## History rows versus #1848 and #1928

#1848 (`6872554ac85`, in this base) provides `ReadCompletion::from_context`
over one original command Read root. It adds guarded page admission bound to
the exact leg and physical Attempt, timer-issued next legs under the root
deadline, and `http_retry_context` for same-leg ReadFailure retries. #1928
(`RUST_HISTORY_PLAN_AUTHORITY.md` on `origin/feat/rust-history-plan-authority`,
not landed) adds Plan/Slot custody with declared page credits and a Read
accepted-prefix recovery terminal.

Bitget history is hand-rolled: its own `rows`/`seen`/`cursors`, with no shared
`HistoryRows`. Failure is strict (no accepted-prefix recovery). The geometry is
up to 1000 30-day windows × `MAX_PAGES` 1000 pages, three timer retries
(250/500/750 ms), 100 ms page spacing, a 10 s detection wait and a mid-query
40085 promotion that re-detects and re-issues.

| Line | Function | Fingerprint | Class | Needs |
|---|---|---|---|---|
| 244 | `start_query` | `23ef96f6` | entry refusal (no credentials) | neither; admit + `fail_context` |
| 249 | `start_query` | `0c9d5d0f` | entry refusal (client-order-id) | neither |
| 263, 281 | `start_query` | `32a96a9d` ×2 | entry refusal (invalid range) | neither |
| 291 | `start_query` | `461218ae` | entry refusal (> 1000 windows) | neither |
| 327 | `query_request` | `b9abde2a` | unreachable: `start_query` already refuses `OrderByClientId` | delete |
| 820 | `query_response` | `5ecc36ad` | strict fail, cursor did not advance | #1848 *if* one root bounds the whole query |
| 827 | `query_response` | `5659fbaf` | strict fail, page cap | same |
| 847 | `query_response` | `755a658b` | final done | same |
| 848 | `query_response` | `d5e898e0` | encoder failure | same |
| 853 | `query_failed` | `ea05be25` | strict fail after 3 retries | same (retries as timer-issued legs or `http_retry_context`) |
| 869 | `history_timer` | `7ab67768` | detection deadline from timer | same (terminal-from-timer uses root validation) |

Conclusion:

- None of these needs #1928's recovery terminal, because Bitget fails strictly.
- **#1848 alone can convert every post-send row**, but only if one Read root
  bounds the whole query. A root's `timeout_ms` would then become a
  caller-visible whole-query deadline over an otherwise unbounded walk (up to
  10⁶ pages). #1848 explicitly forbids guessing that bound. If the native owner
  cannot fix one, the walk needs #1928's Plan with declared page credits.
- The five `start_query` refusals need neither API technically. Their
  admission shape (single root versus Plan Preflight) must match whichever
  authority owns the query, so they move with it.
- PublicTrades is one unsigned page with no cursor and no windows. It fits one
  20 s root trivially, but it shares every fingerprint with the private
  queries, so converting it alone removes nothing.

**Proposed follow-up slice (history, #1848 only):**

- The owner sets one explicit whole-query deadline. The candidate is a fixed
  cap, for example 120 s, or a bound derived from windows × pages that is then
  capped.
- The slice admits a Read root in `start_query`, refuses the five entry cases
  through it, and deletes the unreachable 327 arm.
- Each page goes on a timer-issued leg under the root, and admission goes
  through the `ReadCompletion` guard.
- Retries use `http_retry_context`.
- 40085 promotion re-issues on a new leg of the same root.
- All terminals go through the root, and the History `on_sign_error` arm moves
  to `fail_context`.
- That removes all 12 history rows plus lib.rs `c873db6f` and the streamed
  `a921be85` (history is its last caller). `6c77d608` then stays only for the
  write slice.
- Without an owner-fixed deadline, the slice waits for #1928.

## REDs and controls (original, unchanged production)

All of these are in `sdk-rs/venues/bitget/tests/caller_adoption.rs`. They drive
`Engine<BitgetFutures>` through the `Harness`, which uses `prepared_actions`
(a pass-through today). They assert observable actions only and are not ignored.

Genuine REDs. Each fails today on an assertion:

1. `unanswered_candle_read_fails_at_the_shared_deadline`:
   `assertion left == right failed: an unanswered candle read needs exactly one
   terminal by the 20 s shared deadline; left: 0, right: 1`.
2. `candle_reads_beyond_shared_capacity_are_refused_before_io`:
   `assertion left == right failed: the candle read beyond shared capacity must
   be refused before any HTTP; left: 257, right: 256`.
3. `unanswered_identity_read_fails_at_the_shared_deadline`:
   `assertion left == right failed: an unanswered identity read needs exactly
   one terminal by the 20 s shared deadline; left: 0, right: 1`. After the fix
   it also pins kind Timeout, one equal-text diagnostic and an inert late reply.
4. `identity_reads_beyond_shared_capacity_are_refused_before_io`:
   `assertion left == right failed: the identity read beyond shared capacity
   must be refused before any HTTP; left: 257, right: 256`.
5. `unanswered_balance_read_is_retired_by_the_shared_deadline_and_polled_again`:
   `an unanswered balance read must be retired by the 20 s shared deadline so a
   later poll reads the balance again`. Today `balance_in_flight` latches
   forever. The fixed version also requires that no re-read happens before the
   deadline.
6. `unanswered_settings_probe_ends_at_the_shared_deadline_into_classic_detection`:
   `an unanswered settings probe must end at the 20 s shared deadline and take
   the existing transport-failure route (Classic detection)`. After the fix, a
   late UTA reply to the retired probe must not open the v3 socket or issue UTA
   reads.

There is no trade-entry RED, as in #1938. The text, kind and outcome are
identical either way, and the Context route is proven by the checker prune.

Controls, passing now and required after:

- `candle_read_is_exact_public_unsigned_egress`: exact URL, header list `[("X-CHANNEL-API-CODE","Tealstreet")]`, no body or route, row shape.
- `candle_failures_keep_text_kind_and_outcome`: unsupported 8h (no HTTP), 500,
  envelope 40001, bad row, missing list and transport failure. Each gives one
  hard failure with exact text, kind and outcome `None`, and no diagnostic.
- `identity_read_keeps_read_key_route_and_failure_shape`: read key and
  passphrase, primary without a read key, exact URL with no route or body,
  result JSON, HTTP 500 and transport texts with an equal diagnostic, and the
  public-only refusal before HTTP.
- `classic_account_reads_keep_exact_urls_and_the_read_key`: probe, positions,
  balance, orders and plan URLs, read key throughout, and a full snapshot load.
- `uta_account_reads_keep_their_routes`: UTA URLs, no v2 reads, and the v3
  private socket.
- `degraded_balance_recovery_keeps_the_primary_key_and_route`: a write checked
  without a private payload re-reads balance with the primary key on primary
  egress (`route == Primary` today, `RoutingIntent::Primary` after), while
  positions keep the read key. The write is signed with the primary key.
- `trade_entry_refusals_keep_text_and_not_sent`: no credentials, Bybit
  controls and unknown market each give exact text, kind `None`, `NotSent` and
  no HTTP. A later same-symbol placement writes.
- `late_replies_after_a_terminal_are_inert`: repeated or failed replies after a
  candle or identity terminal add nothing.

Kept green: the existing `identity.rs`, `public.rs` candle tests,
`startup_auth.rs` (probe backoff, 40009, rate-limit pause), `account.rs`
40085 generation fence, and `secondary_policy_tests`.

## Checks (this branch)

These were run from `sdk-rs` with
`PATH=$HOME/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin:$PATH CARGO_BUILD_JOBS=2`:

- On `58af6be9f0e` (REDs only): `cargo test -p safe-cex-venue-bitget
  --no-fail-fast` gives 262 passed and 6 failed, exactly REDs 1–6.
- After the implementation: 268 passed, 0 failed (independent review agrees),
  and `cargo test -p safe-cex-venues --features bitget` passes.
- `cargo fmt -p safe-cex-venue-bitget --check` passes.
- `cargo clippy -p safe-cex-venue-bitget --all-targets -- -D warnings` is clean.

From the worktree root:

- `GITHUB_BASE_REF=feat/rust-bitget-signer-migration python3
  sdk-rs/scripts/check-request-context.py` passes.
- `check-request-context.test.py` passes (15 tests).
- `check-signing.py` passes.

## Implementation order

1. Switch `identity.rs` and the other direct-drain tests to `prepared_actions`
   (pass-through).
2. Steps 1–5 above, then prune the manifest to the actual scan (the delta
   above).
3. Add Bitget caller rows to `RUST_CUTOVER_CALLERS.md` for candles, identity,
   account-read deadlines, and the managed preparation intents.

## Open questions for the owner

1. **Account reads in slice 1?** They give zero inventory delta but empty
   `send` of account callers, and they fix the forever-latched poll and probe
   (REDs 5–6). OKX converted its config read the same way. The recommendation
   is to include them.
2. **Typed Timeout for candles and identity.** The native shape is already a
   hard `CommandFailed`, so passing the engine's `VenueError` through
   `on_http_failed` (kind `Timeout`, as OKX FetchLeverage does) is the
   recommendation. The alternative is message-only through `on_request_failed`.
3. **Probe deadline means Classic.** An engine Timeout reuses today's
   transport-failure route (Classic detection unless UTA is confirmed), which
   is what a host transport timeout does now. The recommendation is to
   preserve it. The alternative is to treat a deadline as non-evidence and
   back off.
4. **Lexical `matches!(request, ..)` rows (6).** Renaming the local binding
   prunes them with no behaviour change. Should that go in this slice, or stay
   frozen?
5. **Discovery `get` (`a5c4555a`).** Converting the public contracts/tickers
   startup reads under `PublicRetries` is a separate slice. Is it wanted next,
   or after history?
6. **History deadline.** Will the owner fix one whole-query bound so the
   history slice can use #1848 alone, or should history wait for #1928?

## Decisions (claude-4yjlw0, slice owner)

1. The account-cycle reads (settings probe, positions, balance, order pages)
   are in slice 1 as Background Read Contexts. They prune no rows but move
   every account caller out of the #1915 send funnel.
2. Candle and identity reads that hit the shared deadline fail with a typed
   Timeout through `on_http_failed`. Today they have no deadline at all, so no
   caller-visible shape is changed; this matches OKX #1938's leverage read.
3. A settings-probe deadline takes today's transport-failure route into
   Classic detection.
4. The six `matches!(request, …)` rows are NOT renamed. They are pruned only
   by real conversion; renaming a binding to dodge the scanner is not allowed.
5. The discovery `get` comes in a later slice.
6. The history whole-query deadline is an owner decision (asked of
   claude-ddi2il). Until it is decided, the history slice waits for #1928.

## Review follow-ups

- **Pre-send probe deadline:** if signing or host bind does not finish within
  20 s, the engine routes through `on_sign_error`. The probe then backs off
  (`probe_failed`) and is NOT Classic evidence, and identity fails Timeout /
  NotSent. Decision 3's Classic route applies only once the probe was released.
- **Capacity exhaustion:** admission precedes the native entry checks. With 256
  outstanding managed reads, an unsupported interval or a trade preflight
  refusal returns the typed Capacity failure instead of its native text.
- **Unreachable funnel filter kept:** the degraded-balance filter in
  `signing_identity` is unreachable now that the send funnel carries only
  history and writes. Removing it leaves `path` unused, and dropping that
  parameter changes the staged `sign_http` call, which renames a fingerprint in
  #1915's owner-approved staging entry. It goes, with its funnel-only test and
  the `secondary_tests` module, when the history/write conversions delete the
  funnel. The degraded-balance primary key is covered on the managed path by
  the `caller_adoption` control.
