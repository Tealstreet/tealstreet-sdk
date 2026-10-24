# Bitget caller adoption, slice 3: the trading write job

Plan first, 2026-10-25; production is held on the gates below.
Owner: claude-4yjlw0. Scope approved by claude-ddi2il.
Branch `feat/rust-bitget-trading-context` from landing `49f7d1857b6`. Slice 1
(#1972) is landed; slice 2 (#2077, discovery) is independent and touches
`lib.rs` only.

## Scope

The remaining Bitget HTTP write funnel: the serial trading job in
`venues/bitget/src/trading.rs` (`resume_trading`, `issue_trade`, the response
handler, `trade_fail`, `trade_timer`), and the body branch of the private
`send` funnel in `lib.rs`. History (#1977), discovery (slice 2), sockets and
the account cycle are out of scope.

### Inventory

`check-request-context.py` `fingerprints()` at the base:

| File | Site | Fingerprint | Occurrences |
| --- | --- | --- | ---: |
| `trading.rs` | `resume_trading` done terminal (:474) | `d4eadb4e` | 1 |
| same | read/write classification `matches!` (:488) | `3e008d0d` | 1 |
| same | amend response `matches!` (:1672) | `9cb2980b` | 1 |
| same | strategy cancel `matches!` (:1787) | `2aa595c9` | 1 |
| same | place-effect `matches!` (:1948, :1961) | `ff399ff5` | 2 |
| same | `trade_fail` terminal (:2196) | `5f67f7ee` | 1 |
| same | `trade_timer` queued-command failure (:2255) | `29da08f8` | 1 |
| `lib.rs` | `send` body branch | `5b441164` | 1 |

Expected: `trading.rs` 7F/8O → 0F/0O, and `lib.rs` drops `5b441164` only.
History still calls `send` (`history.rs:418`), so `a921be85`, `6c77d608` and
`c873db6f` stay until #1977. The `Pending::Trade` `on_sign_error` arm goes with
the funnel. Shrink `STAGING_BOOTSTRAPS['venues/bitget/src/lib.rs']` and its
approved test row in the same implementation; no bound may permit the deleted
branch to return. `secondary_tests.rs` drives `send` with a body and is
rewritten against the managed path. Credential and signing inventories must
not grow.

## Admission and authority

The consumer pattern is shared with OKX: cite
`RUST_OKX_ORDERED_WRITE_CONTEXT.md`, section "Admission and authority", and
the canonical streamed contract in fe7lgm's `RUST_PLAN_SERIAL_STREAM.md`
(`feat/rust-plan-serial-stream`). Constructors are those of the reviewed
engine PR; this plan invents no names. Bitget-specific:

1. At command entry, `begin_preflight(WriteAfterReads, read <Read-role
   selection>, write Primary)`. Native signs every in-job GET (Metadata,
   ResolveAmend) with the Read role, the Secondary read key when configured
   (`signing_identity`, account.rs:181-202), and mutations with Primary.
   In-job reads must keep that key; signing them under the mutation
   credential would move read traffic onto Primary (asked of fe7lgm). The job's metadata, position and amend-identity reads run as
   preflight reads or as read calls of the same streamed owner (gate 2).
2. Keep native entry precedence (`trade_command`, :247-277): missing
   credentials refuse first, then input validation (`preflight`) refuses,
   both NotSent and immediately, before the queue. Only a valid command is
   queued, and only then can the detection wait (:416-418) or metadata reads
   run. A valid-credential invalid input never waits for mode detection.
   Then plan the full native job without publishing Store, progress or I/O.
3. Admit one command-owned streamed serial Plan: one live call, 20 000 ms per
   call, attempts one, Never, except the 40009 grant below. No step cap.
4. Only after admission apply native optimistic status (`Canceling`, amend
   marks). Send each call through `request_signed_managed` with the existing
   Bitget Primary dispatch recipe: exact URL, body and the
   `X-CHANNEL-API-CODE` header (`sign/dispatch.rs:132`).
5. Validate exact delivery before decode, Store changes or accepted IDs.
   Preserve native placement-identity receipts (`placement_identities`,
   :2476-2511) after admission and before each release.
6. Completion via `done_plan` with `job.ids`; failure via the Plan's failure
   authority. No venue timer creates admission or DATA rights.

The Bitget crate opts into `http-plans` at implementation time. Its WASM cost
is measured first (gate 5).

## Native behavior to preserve

- **Queue.** One strict serial queue, one active `Job` (:365-470). Commands:
  PlaceOrders, CancelOrders, CancelSymbolOrders, AmendOrder, SetLeverage,
  SetMarginType, SetPositionMode (`plan_job`, :543-740).
- **Placement.** Orders split at the native per-order max up to 10 000 lots
  (:838-846); a caller client ID cannot split. Classic batches 50 rows per
  symbol (:867); UTA sends one call per lot (:851-856). Algo orders are one
  call each. `acknowledged.len() >= 4096` refuses new placement until
  reconciled (:546); that is a native limit and stays.
- **Cancellation.** Classic batches 50 (:1259); UTA one call per order. The
  idempotent strategy-missing message clears the error (:1669-1670).
- **Amend.** Modify, with a cancel-and-replace fallback (:1458-1480); UTA adds
  a rollback place (:1442-1447). Identity resolution issues up to four GETs
  (`ResolveAmend`).
- **Position read.** Not a job call: the job calls the account cycle's
  `refresh_positions` and parks (`waiting_positions`, :450-457) until that
  borrowed Background read lands, with no native bound (:426-433). It maps to
  an unbounded condition wait before admission.
- **Reads in the job.** Both retry at most three times at 250·n ms
  (`retry_read`, :2245-2252), with different triggers:
  - Metadata retries a transport failure only (:1536-1544). An HTTP non-200
    or malformed metadata body fails the job (:1574-1580, :1614-1620).
  - ResolveAmend retries any not-ready response: transport, non-200, non-zero
    code or absent row (:1630-1640).
  So the engine retry must cover exact `HttpFailed` custody for transport
  retries, not DATA only.
- **Accepted IDs.** Appended as received, no dedup: `success_list` order IDs
  with empty strings filtered, else one orderId, else clientOid
  (:1739-1754); amend pushes the new ID (:2020, :2081). `done` serialises
  `job.ids` (:474). A non-empty `failure_list` keeps the success IDs and fails
  the job (:1756-1760).
- **40009 (signature).** Resent once per call, never twice (`signature_retry`,
  :1652-1658). The check reads the parsed body code with no HTTP-status
  guard, so a 408/5xx body carrying 40009 also resends natively. If the
  resend's own preparation or signing then fails, the failure is Unknown,
  never NotSent, because the first attempt reached the venue (:2175-2178).
- **40085 (UTA required).** Promotes to UTA, restores `Canceling` rows, and
  replans the whole job only when `job.ids` is empty (:1501-1534). After
  partial success it fails with the acknowledged IDs. It is checked on the
  body code before HTTP-status classification, so any status carrying 40085
  promotes.
- **Unknown.** Network, 408, 5xx, malformed bodies and codes 40010, 40725 and
  45001 are Unknown (:1643-1650, :1928-1946), after the 40085 and 40009
  checks above. Reconciliation is
  `refresh_orders` plus the write note. `trade_fail` drops Rejected and NotSent
  once any effect landed (:2196-2203).
- **Detection wait.** If Classic/UTA mode is unknown, the queue waits 10 s
  (:417-418) and `trade_timer` fails queued commands (:2255-2267).
- **No overall bound.** Native has no whole-job deadline and no wait between
  mutation calls.

Worst case for one 10 000-lot order: 200 calls on Classic, 10 000 on UTA. The
whole command has no input-count cap; request count is
`sum(ceil(L_classic / 50) per symbol) + L_uta + A + amend/cancel calls`.

## Overseer rulings (2026-10-25)

1. No step cap, and no root wall-clock: stack on fe7lgm's streamed serial
   Plan (`RUST_PLAN_SERIAL_STREAM.md`, canonical pin `34b0eb6233c`, which adds
   successful-Read and failure-only delayed Read custody; consume the eventual
   reviewed source). Each call is bounded
   by `now + timeout_ms`; the chain advances only from DATA. Native has no
   whole-job bound, so a 10 000-call UTA order stays native.
2. 40009: `SerialCall.attempts` 2 for mutations; the one resend goes through
   `plan_resend_context` from that call's DATA, a fresh retry Context that
   re-runs Prepare/Sign/Bind (new stamp, nonce reserved at Bind). Never more.
   `read_retries` stays venue job state.
3. 40085: the replan is a plain continuation (`continue_serial_plan` with the
   re-read calls, then the replanned job). No seal, no second root or
   admission.
4. Detection wait: a scoped engine wait on a venue condition with the native
   10 s venue bound, never a receipt or root cap (fe7lgm plan item, after
   dazzy3 R6). Detection completes in a borrowed account-read callback
   (`private_detected`, account.rs:286, from the Settings probe in
   lib.rs:795-806, 960-968, 1005-1012); expiry fails the job and the queue
   NotSent (trading.rs:2255-2267). It is needed both before admission and
   between calls of a live Plan: `promote_uta` (account.rs:1041-1052, also
   reached from history.rs:771 and account.rs:658) clears detection mid-job.
5. Reads in the job are `SerialCall` Read operations under the admitted
   credential; native read retries use `continue_serial_plan_after(250·n)`
   on the Plan's own timer. The Unknown reconcile mirrors OKX.
6. Mixed batch: `KnownPartial` plus `fail_plan` projects failure with every
   streamed accepted ID, that batch's included.
7. One scratch CI WASM measurement with `http-plans` enabled (gate 3).

8. Queue: each command queued behind the active job is its own
   WriteAfterReads Preflight, parked with no I/O (`park_preflight`) and
   woken with `request_preflight_ready` when the active job ends, FIFO in
   the same turn. There is no engine bound and no receipt binding, so a
   parked command survives a 25-minute job. On detection expiry the venue
   wakes each queued Preflight and fails it NotSent in its ready custody
   (fe7lgm RED12). A re-park after promotion happens in ready custody.
   Capacity (ruled 2026-10-25: accept and disclose, no exemption): each
   parked Preflight counts against the engine-wide 256 concurrent commands,
   so the 257th concurrent command is refused Capacity, NotSent, before any
   send (fe7lgm condition-wait RED14, `df66105f30e`).

## Gates before production

1. **Streamed serial Plan** (fe7lgm), landed with the contract above,
   including the WriteAfterReads command wait (`b2b40eb5a00`).
2. **#1956 (6b).** Shared journal owner; stack on its landing.
3. **WASM.** Bitget runs under the 1 363 968-byte override. Scratch run
   `40114957320` (`875b60c5005`, only `http-plans` enabled on the bitget
   crate, no venue use) measured **1 418 022** bytes, 54 054 over. The
   slice cannot land under the override as is; a budget ruling or engine
   size work is required before production.

## Callers (tealstreet-next)

TS-visible results are unchanged except two new bounds:

- Each write and read call ends at 20 000 ms as a typed Timeout (Unknown for
  a released write, which then reconciles). Native trading calls had no
  request deadline.
- More than 256 concurrent commands (the active job plus queued writes) are
  refused Capacity, NotSent, before any send. Native queued without limit. Recorded in
`RUST_CUTOVER_CALLERS.md` at production time.

## Original REDs

New file `venues/bitget/tests/trading_context.rs`, driven through
`test_support::prepared_actions`, run against the unchanged base, zero
ignored.

1. A Classic PlaceOrders call is released through managed preparation with
   exact URL, body and Primary headers. Today it is a legacy signed request.
2. Optimistic `Canceling` is not applied before admission: a refused
   admission leaves Store unchanged and fails NotSent.
3. A 40009 reply is resent exactly once through the same owner; a second
   40009 fails Rejected.
4. An unanswered write call ends at its 20 s deadline as Unknown and
   reconciles; nothing after it is sent.

GREEN native controls:

- Classic 50-row batching and UTA one-per-lot for a split order, with every
  ID in order.
- 40085 with no accepted IDs replans as UTA; after partial success it fails
  with the IDs.
- A mixed `failure_list` keeps success IDs and fails.
- Codes 40010/40725/45001 and 5xx are Unknown and reconcile without resend.
- Entry precedence: missing credentials win over invalid input; invalid
  input refuses immediately while the account mode is still undetected.
- Status/code corners: HTTP 500 with body 40009 resends once; HTTP 500 with
  body 40085 promotes and replans. If the managed path cannot preserve
  either, it needs an explicit overseer disposition, not a silent change.
- A 40009 resend whose preparation fails ends Unknown.
- Metadata HTTP 500 fails without retry; metadata transport failure retries
  three times; ResolveAmend retries a non-ready 200.

Source-stage gates, not original claims: the 10 000-call UTA run, the
detection-wait parking, amend fallback and rollback under the streamed
owner, and the read retries after admission.

Reviewers: codex-ih35js for shared-pattern parity, claude-fe7lgm for the
engine contract. Scoped bitget tests and strict checks only, with
`CARGO_BUILD_JOBS=4`.

## Executed original checkpoint

`venues/bitget/tests/trading_context.rs` against the unchanged production
base: 4 assertion REDs, 6 GREEN, 0 ignored. Strict scoped Clippy and fmt
pass (`CARGO_BUILD_JOBS=4`).

- REDs, each failing for its intended reason:
  - `classic_place_is_released_through_managed_preparation`: no managed
    Primary preparation (legacy signed request).
  - `cancel_beyond_shared_capacity_is_refused_before_canceling_status`: with
    the registry saturated, native still sends the cancel.
  - `a_signature_rejection_is_resent_once_through_managed_custody`: the
    native resend is a legacy request.
  - `an_unanswered_write_ends_at_the_shared_deadline_as_unknown`: native has
    no deadline, so no terminal by 20 s.
- GREEN native controls: credentials before input validation; invalid input
  refused with no detection wait while the mode is undetected; a second 40009
  is Rejected; HTTP 500 carrying 40009 resends once; a mixed `failure_list`
  keeps accepted IDs and fails (not NotSent or Rejected); 40010/40725/45001
  are Unknown with no resend.
- Still source-stage, not executed here: the 40085 promotion corners (HTTP
  200 and 500), the resend preparation-failure Unknown, the metadata and
  ResolveAmend retry tables, UTA one-call-per-lot at 10 000 calls, the amend
  fallback and rollback, and the detection-wait parking.
