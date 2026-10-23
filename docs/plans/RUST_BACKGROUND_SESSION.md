# Background-session request Context

Owner: claude-55ao7t (lane under claude-ddi2il). Plan-first checkpoint from
landing `7641650e7b1`; no production source changes yet. This extends the
request-context core in [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md):
its `ContextRegistry`, managed prepare/bind/claim lifecycle and signed dispatch.
It adds no executor, signer, retry loop or clock.

## Problem

Venues moving onto RequestContext are blocked on HTTP mutations that belong to
no user command: listen-key create/keepalive, WS token fetches and session logins.
Today `Scope::Background + Operation::Mutation` is rejected
(`engine/src/request_context_registry.rs:87`). Plain Background also has no
owner-level retirement: its generation is the per-context leg ID (registry
~116), so "retire all of this session's work" cannot be expressed. Each
alternative has a concrete fault:
- Relabelling a listen-key create/keepalive as Read lets read-failure retry
  resend a server-state write after Unknown.
- A fake command ID joins an unrelated command's outcome.
- Staying legacy blocks bridge removal before master, and it also blocks
  deleting BingX #1779's temporary `sign_http` stage.

## Inventory (landing `7641650e7b1`)

All 15 flows use legacy `cx.request`. None sends an HTTP teardown: no Rust
venue issues listen-key DELETE. `Legacy` is the
`request-legacy-sites.json` count for the issuing file(s). It is conservative
and includes other calls.

| Venue | Site | Request | Purpose | Signing | Native fencing / recovery today | Legacy |
| --- | --- | --- | --- | --- | --- | --- |
| Binance USD-M/COIN-M/PM | `binance/src/account.rs:365-407` (`cx.request`+`cx.sign_http`) | POST `/{f,d,pa}pi/v1/listenKey` | create | livenet API key header only; testnet HMAC | no epoch; failure backs off 5 s doubling to 60 s; every reconnect POSTs again | account 2, lib 5 |
| Binance | `account.rs:578-580` timer | PUT same | keepalive every 30 min | same | failure retries every 60 s, no teardown | — |
| AsterDEX | `asterdex/src/account.rs:413-429` → `request.rs:230` | POST `/fapi/{v1,v3}/listenKey` | create | V1 HMAC / V3 standard EIP-191 (EIP-712 is Code-only) + `X-MBX-APIKEY` | `key_epoch`; `key_pending` single flight; failure → `close_private` + Supervisor retry | account 2, request 33 |
| AsterDEX | `account.rs:978-989` | PUT same | keepalive 30 min | same | `is_read` relabel (`request.rs:77`); network retry ×3; PUT retried on 429/5xx | — |
| Bitrue | `bitrue/src/account.rs:513-530` → `lib.rs:333` | POST `…/user_stream/api/v1/listenKey` | create | HMAC headers built in venue | `account.generation`; `listen_pending` single flight; malformed key → `restart_private`; any request re-signed once on signature refusal (`lib.rs:910`) | account 12, lib 22 |
| Bitrue | `account.rs:796-809` | PUT `…/listenKey/{key}` | keepalive 30 min | HMAC | failure → full private restart; POST relabelled safe (`lib.rs:917`) | — |
| BingX | `bingx/src/account.rs:426-441` → `lib.rs:105` | POST `/openApi/user/auth/userDataStream` | create | API key header only | `epoch`; `key_in_flight` single flight; failure → invalidate + 30 s retry | account 21, lib 10 |
| BingX | `account.rs:601-613` | PUT same `?listenKey=` | keepalive 30 min (armed at WS open) | API key only | 404/expired → fresh key; other failure → 30 s retry | — |
| KuCoin futures | `kucoin/src/lib.rs:345-376` | POST `/api/v1/bullet-{private,public}` | WS token per (re)connect | KC HMAC + passphrase (public signed when keyed) | `generation` + `token_pending`; failure → `reconnect_later` | lib 10 |
| KuCoin spot | `kucoin/src/spot/mod.rs:255-286` | POST same | WS token per (re)connect | KC HMAC (private) | same pattern | spot/mod 20 |
| Kraken spot | `kraken/src/spot/account.rs:365-378` | POST `/0/private/GetWebSocketsToken` | WS token per open | nonce HMAC-SHA512 | socket/open fence; non-Trade relabel retry ×3 | 4 |
| GRVT | `grvt/src/account.rs:810-832` → `lib.rs:306` | POST `edge…/auth/api_key/login` | session cookie login | API key in body | `account.generation`; counted read in `on_request_failed` (×5); 401/403 fail closed; reactive refresh | account 12, lib 19 |
| Paradex | `paradex/src/account.rs:475-509` | POST `/auth/{subkey}` | JWT fetch/refresh (180 s, lazy) | Starknet signature (local or delegated `sign_request`) | `auth_in_flight` single flight; 2 s retry | account 10 |
| Polymarket | `polymarket/src/account.rs:394,548,791-802` | GET `/time`, GET `/auth/derive-api-key` | derive L2 creds once | L1 EIP-712 | ×3 at 1 s·n | 5 |
| BreakoutProp | `breakoutprop/src/lib.rs:386-400` | POST `/login` | DXtrade session token | username/password body | generation; lazy relogin; 401 retry once | 14 |

MEXC, Binance Spot, Bybit, OKX, Bitget, BloFin, Bitunix, Crypto.com, Phemex,
WOO, Orderly, ApexOmni, Backpack, Kraken futures and WEEX authenticate over WS
only. Coinbase and Lighter mint tokens locally. These are #1771 socket-auth work,
not this lane's. The brief listed MEXC listen keys; current Rust MEXC uses WS
`login` and has none.

Legacy-path observations. These are motivation and controls, not new defect
claims:

- With order locks, legacy `Cx::request` calls `note_write()` for every
  non-GET (`engine.rs:1676`). It only marks unmanaged placing scopes
  (`order_locks.rs:232-237`). Any managed migration removes it; the
  no-`note_write` test stays.
- Binance has no explicit listen-key single flight. Its POSTs are serialized
  only by the trigger graph: `start_account` once, a Supervisor reconnect after
  a close, and `listen_key_timer` guarded by `socket.is_none() &&
  !is_waiting()`. Binance POST also returns the existing key. Review found no
  reachable stale-key race. Generation retirement replaces the implicit guard
  with an explicit one; that is a hardening control, not a bug fix.
- Four venues relabel session writes as reads to get retries (AsterDEX,
  Bitrue, GRVT, BreakoutProp). Their retry counts and status rules differ per
  venue for the same semantic. Kraken's token relabel matches the Read class
  below.

Native session timers to delete in Phase C (temporary state, tracked here):

| Venue | Timer |
| --- | --- |
| Binance | 30 min keepalive (`account.rs:22`); create backoff 5 s doubling to 60 s (`:23-25`); keepalive-failure retry 60 s |
| AsterDEX | 30 min keepalive (`account.rs:750`); network retry 1/2/3 s (`request.rs:342`) |
| Bitrue | 30 min keepalive (`account.rs:29`) |
| BingX | 30 min keepalive armed at open (`account.rs:23`); 30 s create/keepalive retry |

## Contract

A session is an engine-issued, generation-bearing **owner** alongside commands.
It is not a command, a read or a socket epoch.

- `Owner::Session { generation }`. `Cx::open_session(SessionPolicy) ->
  Result<Session, Error>` issues an opaque, issuer-bound handle. The generation
  is a monotonic engine identity that is never reused, so a reopened session is
  a new generation. There is no public `Scope` variant: contexts come only
  from `Cx::session_context(&Session, …)` or a session parent, so a numeric
  scope can never forge session authority. `SessionPolicy` holds:
  - an optional non-secret `Credential` (slot + `KeyRef`). This is `Some` for
    keyed KuCoin public bullets, which are signed when keys exist.
  - `repeat_safe: bool`, explained under Outcome and recovery.
- **Retention.** The engine keeps no engine-side store of keys, listen keys,
  tokens, cookies or JWTs. The only copy it holds is the attempt's own request
  and unsigned recipe (`http_lifecycle.rs:148-154`), which may carry a listen
  key in the URL (Bitrue path, BingX query). That copy is redacted in Debug and
  dropped when the attempt settles or is retired. The returned session token
  stays in native venue state.
- `Cx::session_context(&Session, Policy)` issues Read or Mutation contexts for
  a live current generation only. The policy credential must equal the
  session's. Every logical session operation captures its own original raw
  bound: an explicit 20 s per flow, matching the existing Background adapters,
  where legacy sent `timeout_ms: None`. A session itself has no deadline, so
  retries and continuations can never extend an operation's bound.
- **Operation class follows effect, not method:**
  - Mutation: listen-key create/keepalive (it creates or extends server session
    state) and GRVT/BreakoutProp logins.
  - Read: KuCoin bullets, Kraken `GetWebSocketsToken`, the Paradex `/auth` JWT
    mint and the Polymarket `/time`/derive GETs. These only mint short-lived tokens, under the session
    owner and with `ReadFailure` retry, matching the core's POST-read rule.
  - Each adopting PR cites the per-endpoint fact behind its class.
- `Cx::retire_session(&Session)` invalidates the generation first, then removes
  its contexts. Unreleased attempts are cancelled through the existing
  preparation/signing cancellation and emit no HTTP. Released attempts use the
  same discard as `retire_http_command` (`http_lifecycle.rs:221`): late events
  find no managed entry and are dropped before venue callbacks, reducers or
  auth observation. There are no reconciliation tombstones and no
  `http_reconciliation.rs` change. Re-opening issues a new generation.
- **Native counters.** The session generation replaces native counters for the
  HTTP response fence only. A counter that also fences socket restart (Bitrue
  `account.generation` `account.rs:537`, AsterDEX `key_epoch` `account.rs:945`)
  keeps that socket role until Phase C hands it to the #1771 `Epoch`.
- **Outcome.** Session mutations join no command journal and change no orders.
  `claim_http` skips every command-journal Mutation branch for them:
  - the reconciliation capture and insert (`http_lifecycle.rs` ~506-534)
  - the post-release `http_write_revision` bump and record update (~548-560),
    so a keepalive cannot fence an unrelated command's late reconciliation
  Command mutations keep all of these and their existing default-refusal test.
  The response path gets a session arm alongside `collect_write_response`,
  `release_flag` and `command_mut`; those currently assume a Command owner.
- **Delivery.** The venue receives exactly one current delivery, through the
  existing callbacks: `on_response`, `on_http_preparation_failed` for NotSent,
  or `on_request_failed` for transport loss or timeout after release (Unknown).
  Ordinary session HTTP adds no new callback. HTTP status alone never proves
  refusal; native codecs classify definitive refusals (e.g. BingX expired
  key). The one exception is the provider key import (#1850 P6b, below).
- **Retry and recovery.** There is no new retry policy, and no amendment to
  "Unknown writes never enter generic retry".
  - A session Mutation has one physical attempt per logical operation, with no
    in-operation retry (`register_attempt`'s retry check needs the command
    journal). Re-preparing a NotSent attempt is not a resend. Session Reads
    keep the existing `ReadFailure` retry.
  - A definitive refusal is codec-proven and effects-free (e.g. -1022 signature refusal),
    so it resolves the operation. A new operation may follow.
  - **Unresolved** means a released session Mutation whose outcome is not a
    proven response or a definitive refusal: transport loss, timeout or an
    unclassified body. It stays unresolved until `retire_session`.
  - Recovery after an unresolved write is a new logical operation with a new
    raw bound.
    The engine permits it only when `SessionPolicy.repeat_safe` is set. That
    flag requires a cited venue fact: Binance/AsterDEX POST returns the existing
    key and PUT extends it; for BingX/Bitrue an extra key expires harmlessly.
  - Otherwise, a new mutation context while a session mutation is unresolved is
    refused until `retire_session` and a new generation. BreakoutProp/GRVT
    relogin go this way.
  - Command contexts cannot be issued on a session. There is no generic
    status-code retry and no `is_read` exception.
- **Signing at send time** uses `request_signed_managed` with the venue's
  shared recipe factory. Key-header-only versus HMAC (Binance livenet versus
  testnet, BingX `Present(APIkey)`) is signer-lane data (codex-uufub4 /
  codex-5c57jl / claude-19pt8s for Aster), not session policy.
- **Order locks.** Session contexts never call `note_write` or join a command's
  lock scope.
- **Bounds.** At most 64 live sessions per engine. Session contexts share the
  existing 256-context registry and 256 managed-HTTP slots. The session holds
  one bounded unresolved flag per generation. Capacity refusal returns the continuation and opens nothing.

### Provider key import (#1850 P6b, claude-19pt8s)

Agreed with claude-19pt8s for the Polymarket L2 derived key: the session core
owns *when* to acquire and retire, and the import primitive owns
*installation* only.
- **Classification:** the derive GET (and `/time`) is a session Read
  (deterministic key mint, repeat-safe). `Cx::request_import` accepts only a
  live session-owned Read context; Command and Background contexts are
  `Invalid`.
- **Delivery:** for import attempts only, `on_import` replaces `on_response`.
  Transport loss still goes to `on_request_failed`. Import responses skip
  unproven-HTTP evidence, reconciliation and streaming.
- **Liveness:** the engine checks that the session generation is live before
  routing a body to `Signer::import`, so a late 2xx for a retired generation
  is never installed.
- **Retirement:**
  - Imported refs are bound to `Owner::Session { generation }`.
  - `retire_session` calls `Signer::drop_imported` after the HTTP discard and
    revoke: synchronously for local providers, and as
    `Action::DropImported { generation }` for host providers.
  - Engine disposal retires every live session.
  - The engine refuses (`Unavailable`), before reaching the provider, any
    context whose credential names an imported ref from a dead generation.
- **Readiness:** `on_import` success is where the venue calls `session_ready`
  to release gated owners; failure maps to `session_failed`.
- **Not built yet:** these engine hooks land with the #1850 seam, after #1832
  and #1842. The retire-all-on-dispose and `drop_imported` calls are owned
  here.

### Cadence and socket linkage (Phase C, after #1771 merges)

The keepalive interval, failure backoff and expiry-driven recreation are
duplicated in every listen-key venue (see the timer table above).
- **Engine cadence.** `SessionPolicy` gains a declared refresh interval and a
  backoff reusing `socket::Supervisor`'s jittered policy. The engine owns the
  session timer, scoped to the generation.
- **Token-gated connect.** #1771 names this as a shared prerequisite: "no key
  custody, transient session-token store… report it as a shared prerequisite".
  This plan owns that prerequisite. For every token-gated feed (listen key,
  bullet, WS token), a `FeedId` link is **mandatory**:
  - A #1771 reconnect grant runs the session operation first. Its result
    supplies the connect URL/token to `connect_socket`.
  - A feed-group stop, or retirement for an auth reason, retires the session.
  - A session retirement retires its feed.
- **Single reconnect owner.** No venue that adopts sessions in Phase B may adopt
  the #1771 `FeedId` before Phase C. Each venue therefore has one reconnect
  owner at a time.
- Phase C deletes the native timers listed above. Phases A and B do not depend
  on the #1771 types.

### Session prerequisite gate (owner ruling, 2026-10-23)

claude-ddi2il assigned this gate to the session core, and credential core
(claude-m26ps4) dropped its C7 gate. Here one session request gates N owners:
success re-dispatches each owner on its own request, and failure fans out to
all of them. Sites, from #1821 `RUST_EXECUTION_GROUPS.md` section B:
- Paradex JWT queue (`paradex/src/account.rs` request/refresh/auth_failed)
- GRVT `account.waiting` (`grvt/src/account.rs` account_defer/account_refresh/login)
- BreakoutProp Login/Discover prerequisite (`breakoutprop/src/lib.rs`)

Contract:
- Each waiting owner keeps its own Context and original raw deadline.
- Session success re-dispatches each owner on its own request.
- Session failure or retirement gives each waiting owner a typed NotSent
  pre-send failure. A waiting owner's expiry fails only that owner.
- The bounded wait list lives in the engine, not in per-venue queues.
- Credential core supplies only `SessionPolicy.credential`.
- A GRVT trade re-sent after relogin is a new physical attempt under its
  command journal, never a session replay.
- AsterDEX `awaiting_clock` stays with the clock lane.

API (A2), revised after an independent audit:
- `Cx::session_wait(&Session, owner: &Context, pending: P) -> Result<(), (P, Error)>`
  parks a continuation behind a live session.
  - **Owner:** must be live. It cannot be a Session owner, because a
    Login→Discover sequence waiting on its own session would deadlock; that
    two-stage flow is plain venue sequencing.
  - **Mutation owners:** refused (`State`) while their latest attempt was
    released without a definitive fact. A 401 must be recorded as a definitive
    refusal before a relogin re-send.
  - **Keys and bounds:** waiters are keyed by owner leg, and a second wait is
    `Duplicate`. The registry's 256 live contexts bound them. A refusal
    returns the continuation.
  - **Timer:** one engine timer at the owner's existing raw deadline (already
    parent-capped, never extended). It queues no HTTP.
- `Cx` cannot call venue hooks, so draining returns the waiters to the caller
  in FIFO order. A wait made during handling parks for the next drain:
  - `#[must_use] session_ready(&Session) -> Vec<Waiter<P>>`, where
    `Waiter::{Ready(Context, P), Failed(Context, P, VenueError)}`. A live owner
    is `Ready`, and the venue then issues the owner's own request. An expired
    or revoked owner is `Failed` with Timeout or Revoked and outcome NotSent.
    It is never dropped, because pending values carry venue in-flight
    bookkeeping.
  - `#[must_use] session_failed(&Session, VenueError)` and `retire_session`
    return every waiter as `(Context, P, VenueError)` with NotSent.
- **Outcome:** the forced NotSent is only the waiter's own pre-send fact.
  Command seals still recompute the outcome from the journal.
- **Expiry hook:** expiry is the only engine-initiated callback, through
  `Venue::on_session_wait_expired(cx, P, VenueError)`. By default it delegates
  to `on_http_preparation_failed(.., Error::TimedOut, ..)` with outcome NotSent
  and kind Timeout, so there are no fail-closed panics.
- **Placement:**
  - The `Io` field `session_waits` sits after `pending`/`signatures`, clear of
    #1816's `http_failure_kind` insert.
  - The timer lookup goes in `expire_http_timer`'s not-found branch in
    `http_lifecycle.rs`, so `Engine::handle` is untouched (#1771's insert
    window).
  - The hook goes right after `on_http_preparation_failed`.
  - This departs from Phase A's "no new `Io` field / no `engine.rs`" only
    because pending values are generic over `P`.
- **Not waiters:**
  - WS auth sends (Paradex `send_auth`, GRVT private/proxy auth) are venue
    continuations after `session_ready`.
  - The Paradex sign queue becomes `request_signed_managed`.
  - BreakoutProp issues `request_managed` as soon as each owner drains, with
    its 5/s limit moved into engine `http_pacing`. Re-queuing contexts in a
    venue rate queue would leave them without a timer.
  - GRVT's early return in `login()` during `login_retry`, which is #1821's
    hang, is covered by the per-waiter deadline.

## Phases

- **A. Core (engine only).**
  - **New file** `engine/src/session_context.rs`: session state lives inside
    `ContextRegistry`, with no new `Io` field (#1771 changes `Io`). The `Cx`
    facade is an `impl<P> Cx<'_, P>` block in this file, like `request_managed`,
    so `engine.rs` stays untouched. The module is declared with `#[path]` from
    `request_context_registry.rs`, not in `lib.rs`.
  - **Narrow edits:**
    - `request_context.rs`: `Owner`/`Scope` variants
    - `request_context_registry.rs`: the Background+Mutation branch,
      `issue_bound` owner match, `validate_handle` generation check,
      `register_attempt`/`release_flag`/`command_mut` session arms, and the
      `http_leg_context` scope match
    - `http_lifecycle.rs`: the `claim_http` session exemption from every
      command-journal Mutation branch (capture, write-revision bump, record), the session arm of `collect_write_response`, and retirement
      discard reuse
  - **Coordination:** confirm the exact regions with codex-uufub4 (registry,
    entry/owner modules) and #1808 (`http_preparation_failed` @433) before
    editing.
  - **Tests first (RED before GREEN):**
    - a Background+Mutation listen-key probe is refused today, then succeeds
      under a session owner
    - retirement before release emits no HTTP
    - a released-then-retired response never reaches `on_response`
    - a stale generation and a foreign issuer are refused
    - a session Mutation never gets a second physical attempt in one operation
    - a definitive refusal resolves the operation; an unresolved one blocks a
      non-`repeat_safe` session
    - a session keepalive released between a command write and its late ACK
      does not fence that ACK's reconciliation
    - a non-`repeat_safe` session refuses new mutations until retired
    - command scope is refused on a session
    - the session credential mismatch is refused
    - no `note_write` for session mutations
    - capacity rollback
    - no credential/token bytes in Session/Context/attempt Debug, including
      the URL
    - command mutations still refuse release without a reconciliation codec
- **B. References.**
  - **Binance:** `binance/src/account.rs`, `lib.rs` and the listen-key paths.
    Create and keepalive move to session contexts. The native 30 min
    keepalive and 5–60 s create backoff are kept until Phase C; the timer table
    above lists them. An explicit generation guard replaces the implicit
    trigger-graph single flight. The two `account.rs` listen-key sites are
    pruned from the ratchet. The testnet path uses shared `signed()` in
    `lib.rs:420-441`, so the `lib.rs` counts shrink only if that helper does.
  - **Second reference:** AsterDEX once #1785–#1788 release
    `asterdex/src/account.rs`. Its key_epoch/keepalive recovery is preserved,
    and the `is_read` listenKey exception is deleted. This is coordinated with
    codex-ih35js (#1803) and claude-19pt8s. If AsterDEX is still blocked, use
    Bitrue (HMAC listen key with its own generation).
  - **Gates:** focused engine + venue tests, Binance always, strict
    Clippy/fmt/bans, shrink-only inventory.
  - **Review:** independent correctness/adversarial review answering the
    architecture gate.
  - **Checkpoint** (`47c98d53c78` + `13d3f229329`):
    - **Code:** `Owner::Session { generation }`, `Session`, `SessionPolicy`,
      `Cx::open_session`/`session_context`/`retire_session`/`session_live`,
      and the `claim_http` command-journal exemption. Nothing is public
      beyond those.
    - **Tests:** 22 session tests.
    - **Fault controls:** four compiled faults fail the exact tests (journal
      for every mutation, retirement without discard, no unresolved gate,
      in-operation retry).
    - **Gates:** engine 646 tests with order locks and 627 without, Binance
      293, strict Clippy both ways, fmt, bans.
    - **Review:** the independent review found a context issued before a write
      became unresolved could still send a second write on a session that is
      not repeat-safe, and a venue fact could overwrite an engine-recorded
      failure. Both are fixed, with the review probes as regressions that fail
      on `47c98d53c78`. Re-review is CLEAN; architecture gate No.
    - **Ordering rule:** record the codec fact before `finish_http_context`;
      finishing first leaves the write unknown (fail-safe, pinned).
    - **Not yet:** no venue adopts sessions at this checkpoint.
- **A2. Session prerequisite gate** (above), engine-only, before the Paradex/
  GRVT/BreakoutProp adoptions.
  - **Checkpoint:**
    - **Code:** `Cx::session_wait`, `session_ready -> Vec<Waiter<P>>` and
      `session_failed`; `retire_session` now returns its waiters; plus the
      `Venue::on_session_wait_expired` hook and the waiter timer in the
      `expire_http_timer` not-found branch.
    - **Tests:** 29 session tests in all.
    - **Fault controls:** seven compiled faults fail exact tests (no resend
      gate, dropping revoked owners, no timer clear, early expiry, self-wait,
      no command hold, no retry-policy check).
    - **Gates:** engine 653 tests with order locks and 634 without, Binance
      293, strict Clippy both ways, fmt, bans.
    - **Review:** the independent review found that a parked command could
      lose completion authority once pruned past its deadline. Waiters now
      hold the command owner until it finishes. It also found the resend
      check was looser than `retry`; it now shares `retry`'s journal
      admission and requires the context's refusal policy.
  - **BingX checkpoint** (stacked on #1832 and landing; claude-4yjlw0
    confirmed no overlap with #1884):
    - **Code:** listenKey POST create and PUT keepalive run as repeat-safe
      session Mutations through a thin `managed::session_write` over the
      existing managed `queue()`. BingX listen keys expire 60 minutes after
      their last keepalive, so an extra create or keepalive is harmless. The
      writes are created with `resent: true`, because a listen key presents
      only the API key and legacy never resent it on a signature refusal.
    - **Session lifecycle:** each create and every `invalidate_private` retire
      the previous session. Native `epoch`/`key_in_flight` recovery stays as
      it was.
    - **Ratchet:** `bingx/src/account.rs` goes 17 → 15 (its two listen-key
      callers).
    - **Not removed yet:** `Self::request` stays on landing because
      `fetch_metadata` still uses it; whichever of this and #1884 lands second
      deletes it. The `lib.rs` STAGING_BOOTSTRAPS `sign_http` allowance stays
      until codex-cxxyqr's history Completion bridge removes the history
      callers.
    - **Declared caller change:** an explicit 20 s raw bound. It matters for
      keepalive: a hung create is already dropped by the native 10 s connect
      watchdog, but a hung keepalive runs with no watchdog and previously
      relied on the host default. It now fails at 20 s and the native 30 s
      `RETRY_MS` PUT follows.
      `a_hung_listen_key_keepalive_fails_at_its_bound_and_retries` fires
      timers in real order and fails on the legacy code. Request bytes are
      unchanged: same method, URL, `?listenKey=` query, the per-method
      unsigned headers plus `X-BX-APIKEY`, and no signature.
  - **MEXC:** no session HTTP consumer. Rust MEXC authenticates its private
    socket with a signed WS `login` (confirmed by codex-ih35js), which is
    #1771/signer work.
- **C. Cadence/feed linkage** as above, after #1771.
- **D. Hand-off.** The API and adoption notes go to codex-9czdjh, codex-zs5e83,
  codex-ih35js and codex-hcmq3f. BingX adoption and deletion of the #1779
  temporary `sign::stage` session allowance follow. The final deletion of that
  physical allowance waits for both this and codex-cxxyqr's history Completion
  bridge; whichever lands second deletes it.

## Non-goals and limits

- No Background/Read relabel of session Mutations and no fake command ID.
- No engine-side token store.
- No WS-only auth changes (#1771 owns those).
- No remote-signing wire change.
- No new timeout option. Each flow uses an explicit 20 s `Policy.timeout_ms`.
- No local release WASM, replay sweep or timing gate; CI owns them.
- Architecture gate: **No.** This is the engine choke point for session
  ownership, fencing and recovery admission. It replaces four venue-local read
  relabels and four response-fence counters. Token-gated socket connect, the
  #1771 shared prerequisite, is owned here in Phase C, so it is not split per
  venue.

## Plan review

An independent adversarial review checked 13 inventory rows and their legacy
counts against source and found no missed flows. Corrections it raised, all
incorporated above:
- `IdempotentSession` dropped, replaced by the repeat-safe new-operation rule
- complete Phase A file list
- tombstones replaced by the existing retirement discard
- the Binance race downgraded to hardening
- `note_write` scope narrowed to unmanaged placing scopes
- retention wording corrected
- the #1771 token-gated connect made mandatory
- native timers tracked for deletion
- no `engine.rs`/`Io` edits
- keyed KuCoin public bullets
- the dual-role counters named
- explicit 20 s bounds

A re-review then raised four more points, also incorporated:
- the `claim_http` write-revision branch is exempted too
- session mutations get no in-operation retry, because there is no journal to
  record it
- "unresolved" is defined
- the Paradex JWT is classed Read, and stale tombstone/timeout text was removed
