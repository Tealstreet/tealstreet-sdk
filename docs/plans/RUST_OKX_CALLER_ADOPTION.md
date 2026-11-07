# OKX caller adoption: leverage, account GET and trade entry

Plan, 2026-10-24. Branch `feat/rust-okx-caller-entry` starts from OKX signer PR
#1773 head `c31fcc88e10` (still open), which sits on landing `632da7717e2`.
Landing has since moved to `c55a6a76044` (#1888). This is the first OKX
caller-conversion slice. It covers only sites that need no history bridge
(#1848). Owner: claude-4yjlw0. The OKX socket belongs to codex-5c57jl's #1771
(do not touch socket code). History waits for codex-cxxyqr's #1848.

## Inventory on this branch

The inventory was recomputed by importing `scripts/check-request-context.py`
(`g.sites()`, `g.fingerprints()`). Each fingerprint was mapped to its line by
replaying `tokens`/`receiver_start`/`group_end`. The checker passes, and the
manifest equals the scan: 47 OKX occurrences in 36 hash rows across 4 files.
#1773 moved the account `get` (landing `account.rs` L268, `8bd2847d`) into
`lib.rs`. As a result, the landing-pinned rows in the assignment map are stale
for `account.rs` and `lib.rs`.

In scope (10 occurrences removed):

| Fingerprint | File:line | Function | Class | Removed |
|---|---|---|---|---|
| `1620031f` | account.rs:232 | `fetch_leverage` | entry refusal (unknown market) | yes |
| `d88d43f5` | account.rs:236 | `fetch_leverage` | entry refusal (no credentials) | yes |
| `c5a021da` | account.rs:702 | `account_response` | managed read, success | yes |
| `c873db6f` | account.rs:703 | `account_response` | managed read, parse failure | yes |
| `569abc44` | account.rs:1047 | `account_failed` | managed read, HTTP/transport failure | yes |
| `b1761cde` | lib.rs:396 | `get` | managed read (config + leverage GET) | yes |
| `90571c5b` | trading.rs:161 | `trade_command` | entry refusal (`SetMarginType`) | yes |
| `7d14cc70` | trading.rs:239 | `trade_command` | entry refusal (not ready) | yes |
| `1459ff81` | trading.rs:243 | `trade_command` | entry refusal (already running) | yes |
| `fbafcc48` | trading.rs:264 | `trade_command` | entry refusal (invalid input) | yes |
| `c873db6f` | lib.rs:1405 | `on_sign_error` | sign-failure completion | no: the Leverage arm leaves, but History and Candles keep the site |

Out of scope (37 stay):

- **history.rs, all 18.** These are held for #1848. The bridge-dependent rows
  are 343/344 `finish`, 500/504/517/518/524/534 `start_lookup`, 694/695, 1021,
  1043 and 1072/1073. The `start_query` entry rows 371/384/455 (`2b6c8768`)
  and 447 (`2ef56c72`) need no bridge. They stay with the bridge owner's file,
  to be converted after #1848 or with codex-cxxyqr's agreement.
- **lib.rs, 10: two funnels.** 329 `sign_http` (`2e921890`) and 408 `list`
  (`b8eef2a7`) still serve EEA private instruments, tickers, candles, account
  snapshots, history and closed positions.
- **lib.rs: candles.** These are 742 and 1552 `2b6c8768`, 1342 `c5a021da`,
  1343 `c873db6f`, 1560/1576 `a39517b8` and 1567 `1cb580e2`. They need no
  bridge either. They are the recommended next slice (BloFin/Binance candle
  precedent).
- **lib.rs: 1405 `c873db6f`.** It is kept as shown in the table above.
- **trading.rs, 9: the write path.** These are 797 `request`, 829/835/853
  `done "[]"`, 841/842 and 849/850 `done`/`fail`, and 1079
  `finish_failed_trade`. The ordered batch, algo and settings writes need the
  shared Plan or execution-group write admission plus reconciliation codecs.
- **#1771 socket.** It has no rows in this inventory. Login, ping and
  supervision are left unchanged.

Expected delta: account.rs 5→0 (the path leaves the manifest), lib.rs 11→10,
trading.rs 13→9, history.rs 18→18. OKX goes from 47→37 occurrences and from
36→26 rows.

## Replacement mechanism (landed APIs only, no `http-plans`)

1. **Recipe header seam.** Implement the existing
   `SignStep::unsigned_http_headers` on `dispatch_signing::Http`. It returns
   the static `x-simulated-trading: 1` header for demo aliases and nothing for
   live ones. This is venue canonicalization; BingX already does the same. The
   result keeps the legacy header order: `OK-ACCESS-*` from `apply`, then
   demo. `Http::new` gains the testnet flag. No new signing or crypto lines
   are added, so `check-signing.py` is unchanged.
2. **FetchLeverage.** This follows the WEEX `managed.rs` and Binance
   `read_context` precedent, with admission moved first as MEXC/KuCoin/BloFin
   do.
   - Admit first with `cx.admit_http_command(Policy { operation: Read,
     credential, timeout_ms: 20_000, pacing: None, retry: Never })`.
     - `credential` keeps the existing GET selection. It uses
       `Secondary`/`read_credentials` when that is present; otherwise
       `Primary`; otherwise `None`.
     - If admission returns `Err`, return. The engine already published the
       typed failure.
   - Unknown market and missing credentials then call
     `fail_context(&context, …)`, with the same text.
   - The send is `request_signed_managed(context, Get, url, None,
     Box::new(Http::new(Some(identity), path, testnet)),
     Pending::Account(Request::Leverage { context, … }))`. If the queue
     refuses, call `fail_context`.
   - Response handling:
     - First, `validate_read_http_delivery(&context, &context)`.
     - Then the existing envelope check, `leverage_result` and
       version-fenced store publish.
     - Then `done_context`/`fail_context` in the same callback.
   - Failure routing:
     - `account_failed` (HTTP error and transport failure through the default
       `on_http_failed`) calls `fail_context`.
     - `on_sign_error` (reached through the default
       `on_http_preparation_failed`) gets its own Leverage arm, which calls
       `fail_context`.
     - Without `http-plans` the engine does not auto-seal plain Contexts, so
       every failure path above must complete the context. There is no numeric
       fallback anywhere.
3. **Account config GET.**
   - It runs as a `Scope::Background` Read Context with the same credential
     selection and `RetryPolicy::Never`. It is sent with
     `request_signed_managed`, and `Request::Config` carries the Context.
   - Every terminal path calls `finish_http_context` exactly once before the
     existing logic runs: success, `config_rejected` (documented key-rejection
     codes, 50113 twice), envelope or parse failure, transport failure
     and provider refusal.
   - Startup backoff (`startup::Backoff`), the 50113 two-in-a-row counter and
     `config_unanswered` are unchanged.
   - With both callers gone, delete `Okx::get`. `sign_request` remains for
     `list`.
4. **Trade entry refusals.** Use one helper.
   - It admits `Policy { operation: Mutation, credential: primary or None,
     timeout_ms: 20_000, retry: Never }` and then calls
     `fail_context(&context, error)` with the existing `not_sent(..).kind(..)`
     value.
   - If admission returns `Err`, return.
   - Successful planning still goes to the legacy `next_write`, without
     admission. The entry loan stays Fresh, so numeric completion of the write
     path stays legal until the write slice. Planning helpers borrow `&Cx`
     only, so no ID is reserved before admission.

Architecture gate: **No**. This adopts the shared entry, managed-HTTP and
Context-completion choke points. It adds no venue-local retry, timer, ledger,
clock or admission. The BingX paired ReadOnly Plan (unlanded,
`origin/feat/rust-bingx-paired-metadata`) is not needed, because OKX
leverage-info is a single GET. No shared API is missing for the read
conversions. Trade refusals have no landed Mutation-refusal precedent (see Q1).

Signer interplay: managed sends reuse the #1773 `dispatch_signing::Http`
recipe through the injected provider. Reads keep secondary-key selection.
Writes are untouched and stay on primary. Local, browser and keyless
RemoteSigner providers sign the same `SignStep`. Binance is the landed
precedent for hosted managed signed reads. The registry hosted/alias tests stay
authoritative.

## #1773 STAGING_BOOTSTRAPS (`venues/okx/src/lib.rs`)

- The staged new map adds three items to the old landing map:
  - `2e921890` (`sign_http`)
  - `b1761cde` (legacy GET)
  - one `c873db6f` (`on_sign_error`)
- This slice removes `b1761cde`. The other two stay until candles, closed
  positions, EEA instruments, snapshots and #1848 leave `list` and
  `on_sign_error`.
- The new 10-entry lib.rs map is a subset of the staged map, and the parent
  manifest forbids regrowth. So the entry needs no edit here.
- It cannot be deleted while any compared base still holds the pre-#1773 map.
  Deletion belongs after #1773 lands, by owner decision.

## REDs first (unchanged source), then implementation

First, a harness commit: OKX `tests/support` gains `prepared(engine, now)` over
`safe_cex_engine::test_support::prepared_actions`. Config and leverage fixtures
use it: `ready_engine`, account/products/startup* tests and
`registry/tests/okx_{signer,hosted_signing}.rs`. Legacy actions pass through
it unchanged, and every existing assertion stays GREEN.

Genuine behaviour REDs, expected to fail on the current source:

1. `tests/trading.rs::fetch_leverage_unanswered_read_fails_at_shared_deadline`
   - Issue FetchLeverage 7 with no reply, then fire the timer at +20 s.
   - Expect exactly one `CommandFailed{7}` of kind `Timeout`.
   - A late reply must not change `setting("BTCUSDT")` or add a second
     terminal.
   - Today the command hangs, so this is RED.
2. `tests/startup_auth.rs::unanswered_config_read_times_out_into_startup_backoff`
   - Leave the private config GET unanswered past 20 s.
   - Expect one config GET to be re-sent after the existing backoff.
   - Today `config_in_flight` stays set forever, so this is RED.
3. `tests/trading.rs::leverage_reads_beyond_shared_capacity_are_refused_before_io`
   - Issue 256 outstanding FetchLeverage reads, then one more.
   - Expect a typed capacity failure for the extra read, with no extra HTTP.
   - After one read is answered, the next read is admitted.
   - Today the extra read sends a GET, so this is RED.

GREEN controls, which must pass both before and after:

- Same-text single failures, with no HTTP, for an unknown market and for
  missing credentials.
- Read-key `OK-ACCESS-KEY` on the leverage and config reads, and primary on
  writes.
- The demo header on all four aliases.
- Provider refusal gives one NotSent failure with no HTTP.
- The existing `fetch_leverage_*`, `leverage_read_survives_*`, 50113 and
  key-rejection startup tests.
- `trade_entry_refusals_keep_text_kind_and_not_sent`, covering all four
  branches. After a refusal, a same-symbol PlaceOrders still writes, which
  proves no residual order lock or pending placement.
- A duplicate active ID is refused by the engine guard before the venue runs.

There are no compile/API REDs. The API shape is proven by the checker prune.

Implementation: steps 1–4 above. Then prune the manifest to the actual scan,
and update the OKX caller rows in `RUST_CUTOVER_CALLERS.md`:
- FetchLeverage now has a 20 s shared deadline (Timeout) and a typed capacity
  refusal.
- An unanswered config read now enters startup backoff.
- Reads emit `ManagedHttp` preparation, as Binance/BingX/WEEX already do.
- Trade refusal text, kind and outcome are unchanged.

## Checks

From `sdk-rs` with `PATH=$HOME/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin:$PATH CARGO_BUILD_JOBS=2`:

- `cargo test -p safe-cex-venue-okx`. The baseline here passes trading 39,
  account 27 and products 45.
- `cargo test -p safe-cex-venues --features okx --test okx_signer --test okx_hosted_signing --test hosted_signing`
- `cargo clippy -p safe-cex-venue-okx --all-targets -- -D warnings`, plus
  `cargo fmt --check`
- `python3 scripts/check-request-context.py`, `scripts/check-request-context.test.py`,
  `scripts/check-signing.py` and `scripts/check-bans.sh`

Actual-WASM hosted alias positives and replay remain CI gates.

## Open questions for the owner

1. Trade refusals admit a Mutation owner only to fail it. No landed venue does
   this; MEXC, KuCoin and BloFin refuse under Read admission. The alternative,
   `begin_preflight(WriteAfterReads)` with `fail_preflight`, needs `http-plans`
   and a write credential. That cannot express the no-credential "not ready"
   case. Should we accept this, or defer the four rows to the write slice?
2. trading.rs:243 is unreachable behind the engine's duplicate-active guard
   (`guard_order_mutations` + `order-locks`). Should we delete it instead of
   converting it?
3. Should the balance, positions and orders snapshot `list` reads move to
   Background Contexts now? That gives zero inventory delta. They also keep
   OKX's 5 s account retry timer, which is an existing venue-local retry.
4. Rebase timing: should this branch wait for #1773 to land, or ride on its
   head?

## Owner decisions (claude-ddi2il, 2026-10-24)

1. The four `trade_command` entry refusals stay in this slice. Each admits a
   terminal-only Mutation Context and refuses it NotSent at the owner cap
   (precedent: #1921).
2. Delete the unreachable "already running" arm (`trading.rs` ~243). Add an
   engine test proving a duplicate active command ID is rejected before the
   venue sees it, so the deletion is evidence-backed.
3. Build on #1773's head and re-integrate #1773's actual landing when it lands.
