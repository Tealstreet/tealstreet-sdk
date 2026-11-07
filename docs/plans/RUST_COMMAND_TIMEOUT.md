# Caller command wait in the engine

Owner: claude-fe7lgm. Assigned by claude-ddi2il on 2026-10-24 as its own
engine-window PR, ahead of the Hyperliquid history slice. Reviewers:
codex-uufub4 (engine command decode, entry custody) and codex-5c57jl (host).
Base: landing `ba414853951`. This is a plan; failing tests follow review.

## Why

The overseer ruled that a multi-page history Read root lasts exactly as long as
the caller is willing to wait: the configured `rustCommandTimeoutMs`, or the
per-call override. Today that value never reaches the engine:

- `rust.exchange.ts` validates `extra.rustCommandTimeoutMs` and hands it to the
  WASM `EngineHost` (`hosts/wasm/js/host.mjs:275`), which also accepts a
  per-call `options.timeoutMs` (`host.mjs:384`). The host only uses it to arm
  its own listener timer.
- The command JSON (`{...command, command: id}`) carries no deadline.
  `decode_host_input` and `Event::Command` have nowhere to put one, and no
  `Cx` API can read one.
- Native hosts (`hosts/native/src/lib.rs`, `server.rs`) decode the same
  envelope and own their own waits.

So a venue cannot bound engine work by the caller's wait. Hyperliquid's history
walks keep paging after the caller has given up; nothing cancels them.

## Contract

Revised after uufub4's API-fit review (2026-10-24).

1. **Wire.** For every command that expects a reply, the WASM host adds the
   *effective* wait to the envelope: `{...command, command: id, timeoutMs}`.
   Effective means the per-call override if given, else the configured default;
   it is the same number the host arms its own timer with, validated by the
   host's `commandTimeout()` (positive safe integer, at most 2,147,483,647).
   No-reply controls (`setPollScale`, subscriptions, `publicFeedUpdate`,
   `setTickerSymbols`) send no field.
2. **Decode.** `decode_host_input` returns `HostInput::Command { command,
   timeout_ms: Option<u32> }`.
   - An absent `timeoutMs` is `None`.
   - A present `null`, non-integer, zero, negative or > 2,147,483,647 value is
     a decode error, matching the host's own validation.
   - `decode_command` (tests and tools) keeps ignoring the field.
   - The cold-subscription decoder is unchanged, since subscriptions have no
     reply.
3. **Receipt stamp.** Every *externally received* reply command gets one
   absolute caller deadline at receipt: `now + timeout_ms`, or `now + 120_000`
   when absent.
   - Receipt means the public `Engine::handle(now, Event::Command(..))`
     (native hosts and tests) and the new `Engine::handle_command(now,
     command, timeout_ms: Option<u32>)` (the WASM host). `handle_command`
     itself refuses a non-finite `now`, an out-of-range `timeout_ms`, or a
     computed deadline that is not finite or does not advance past `now`,
     with a typed error and no dispatch. The public `handle` applies the same
     deadline check to its default stamp; on failure it dispatches unstamped,
     so the getter answers `None`.
   - Internal re-dispatch stamps nothing. A held cancellation or deferred
     cancel-all stores the exact receipt it arrived with and dispatches under
     it. A deferred entry therefore sees the remaining time from its own
     original receipt, default included, and never infers a receipt from the
     command id.
   - An engine refusal before the venue's entry (unsupported action, options,
     order-lock refusal) runs inside that command's own entry. A duplicate
     id's refusal therefore cannot clear the live command's stamp. An
     asynchronous terminal outside any entry ends that id's uncarried live
     wait, because host replies are keyed by id.
   - The stamp is keyed by command id with a receipt sequence, its exact
     receipt identity. A receipt whose id already has a live stamp (a
     duplicate live command) does not replace it. That entry has no stamp of
     its own and the getter answers `None`.
   - The stamp is removed on the command's terminal through either emitter:
     the shared `finish_sealed_command`, or the typed initial rejection in
     `with_command_entry` (direct `CommandFailed`). Removal compares the exact
     receipt sequence, so a duplicate entry's terminal can never delete the
     first command's live stamp. A reused id after a genuine terminal starts
     fresh.
   - A stamp that a held or deferred command still carries leaves only with
     that command's own terminal, so the command still reads `Expired` past its
     deadline. A terminal outside any entry cannot remove it, because the
     carrying command has not run.
   - A released batch's later commands count as carried until their own
     dispatch begins.
   - 4096 stamps is the threshold for dropping expired stamps, not a hard cap.
     At that size, expired stamps that nothing carries are dropped, so a venue
     that never answers cannot grow the table without bound. Unexpired
     unanswered waits stay until they expire.
   - The receipt sequence never wraps: once exhausted, a receipt goes
     unstamped and the getter answers `None`.
4. **API.** `Cx::command_wait() -> Option<CommandWait>`, with `enum
   CommandWait { Remaining(u32), Expired }`. It is observation only: it
   grants no send, completion or retry right and never renews anything.
   - It answers only for the current genuine command entry, whose own receipt
     stamp is live, in states Fresh (before admission), Bound or Legacy.
   - It is `None` in every borrowed callback: timer, socket, HTTP delivery,
     plan readiness, another command's entry, a rejected or spent entry, or a
     duplicate entry without its own stamp.
   - `Remaining(ms)` is `floor(deadline - now)` whole milliseconds and is
     always at least 1. A remainder under 1 ms is `Expired`, as is
     `now >= deadline`. `Remaining(0)` is never exposed and nothing renews.
   - On `Expired` in a **Fresh** entry (before admission or any opaque
     effect), a venue must not admit or start work. It calls
     `Cx::reject_expired_command(operation)`, which takes no numeric id and
     adds no legacy site. That emits one typed `Timeout` terminal through
     the same `with_command_entry` rejection as `admit_http_command`, with
     zero HTTP, timer or Store effects. The outcome is `NotSent` for a
     mutation and none (`None`) for a read. `Ok` means that terminal was
     emitted and the command is finished, unlike an admission's `Ok`. It returns `Revoked`, with no
     effect, unless the wait is `Expired` and the entry is Fresh.
   - A **Bound** or **Legacy** entry may already own a Context, Preflight or
     Plan that has sent writes. There, `Expired` only informs the venue. It
     terminates through that existing authority and its journal projection,
     and never claims `NotSent`.
5. **No other effect.** The engine does not enforce the deadline by itself.
   Venues opt in by using `Remaining` as a root `Policy.timeout_ms`. The
   host's own timer and its `COMMAND_TIMEOUT` result are unchanged.

## Consumers

Hyperliquid history (next slice) sets its original Read root to the
`Remaining` wait (failing typed Timeout with zero work on `Expired`) and mints each 20 s page child at queue dequeue. No
other venue changes here. The CALLERS entry distinguishes:

- **Caller abandonment:** the host timer rejects the promise, and the engine
  keeps any in-flight transport.
- **Engine-root expiry:** an adopting venue stops issuing work at the same
  deadline.

It also states the 120000 default for hosts that send no wait.

## Plan consumers: a gap this PR does not close

`admit_plan` derives a Plan's whole deadline from its declaration at
admission (`bound.deadline(now)`). The caller's receipt deadline does not
cap it, and a Plan consumer cannot pass `Remaining` captured earlier (after a
JWT or preflight wait, or on a stage restart) without renewing from a later
`now`. Plan consumers (BloFin, Bitunix and others) need a separate, checked
"original receipt deadline" cap seam in the Plan core before they claim
composition with the caller wait. They must never shrink native step
geometry to fake a cap, nor cast a Plan as a Read root. Read-root consumers
of #1848 (Hyperliquid history) are not affected: they capture the wait in
the Fresh entry and keep that exact root.

## Files

- `sdk-rs/engine/src/command.rs`: header field and the `HostInput::Command`
  shape.
- `sdk-rs/engine/src/engine.rs`: `handle_command` and the deadline stamp.
- `sdk-rs/engine/src/request_context_entry.rs`: `command_wait` beside
  the entry custody it reuses.
- `sdk-rs/hosts/wasm/src/lib.rs`: passes `timeout_ms` from `HostInput` to
  `handle_command`.
- `sdk-rs/hosts/wasm/js/host.mjs`, `host.d.mts`, `hostcommand.test.mjs`: send
  the effective `timeoutMs`.
- `sdk-rs/hosts/native/src/server.rs`: maps the new `HostInput` shape and
  keeps `handle` (no wait).
- This plan, `RUST_CUTOVER_CALLERS.md` and `docs/plans/README.md`.

There are no venue changes and no request-site inventory changes.

## Original REDs (after plan review)

Engine, through real decode and dispatch (a probe venue records
`command_wait()` in its entry callbacks):

1. An envelope with `timeoutMs: 300000` gives `Remaining(300000)` in that
   command's Fresh entry, before any admission. Without the field it gives
   120000. RED today, because the field is dropped and no API exists (the
   adapter returns `None`).
2. A held command released 5 s after receipt (the engine holds at most
   `ACK_WAIT_MS` = 10 s) gives 295000 with an explicit 300000 wait, and
   115000 with the default 120000, so the default is stamped at receipt.
3. `None` in timer, HTTP response, socket and plan-readiness callbacks, and in
   another command's entry.
4. A released Fresh entry at or past its deadline gives `Expired`, and so
   does one under 1 ms short of it (no `Remaining(0)`). The probe's typed
   Timeout terminal makes zero HTTP, timer or Store effects: outcome
   `NotSent` for a mutation command, none for a read. A Bound entry with a
   released write that sees `Expired` ends through its Context/journal
   projection, not `NotSent`.
5. A duplicate live id: the second receipt neither replaces nor reads the
   first stamp. The duplicate's own terminal does not delete it, and the
   first keeps its deadline.
6. Cleanup and reuse: after a typed initial rejection (`CommandFailed` from
   `with_command_entry`) and after a normal terminal, a reused id gets a
   fresh stamp.
7. Decode: absent is `None`; `null`, 0, -1, 1.5, "1" and 2147483648 are
   errors; 2147483647 is accepted. `handle_command` refuses a non-finite
   `now` and an out-of-range wait with no dispatch.
   - The public `handle` at a raw clock where `now + 120000` is not finite
     or does not advance dispatches unstamped, and the getter answers `None`.

API fit: uufub4 CLEAR for decode and entry custody at `de14dd30926`,
including Fresh-only pre-effect expiry, existing authority after admission,
floor and sub-millisecond expiry, a finite advancing deadline, and exact
receipt-sequence cleanup. No getter grants authority. Host clearance belongs
to 5c57jl. Source still needs an adversarial custody review.

Host:

8. `host.mjs` sends `timeoutMs` equal to the per-call override, else the
   configured default, for reply commands only.
9. Existing host deadline tests are unchanged.

## Gates

- Engine tests with and without features; WASM host JS tests; native host build.
- Strict Clippy and fmt on touched crates.
- Both request-context checks unchanged.
- Independent reviews: uufub4 (decode and entry custody) and 5c57jl (host),
  plus an adversarial pass.
- Architecture gate: No. A single, read-only engine fact the host already owns.
  It grants no authority, and no venue changes behaviour without adopting it.
