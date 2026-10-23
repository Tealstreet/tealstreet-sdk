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

1. **Wire.** For every command that expects a reply, the WASM host adds the
   *effective* wait to the envelope: `{...command, command: id, timeoutMs}`.
   Effective means the per-call override if given, else the configured default;
   it is the same number the host arms its own timer with, validated by the
   same `commandTimeout()` bounds (positive safe integer, at most
   2,147,482,647). No-reply controls (`setPollScale`, subscriptions,
   `publicFeedUpdate`, `setTickerSymbols`) send no field.
2. **Decode.** `decode_host_input` reads an optional `timeoutMs` from the
   command header and returns `HostInput::Command { command, timeout_ms:
   Option<u32> }`. A present but non-integer, zero, negative or out-of-range
   value is a decode error, matching the host's own validation. `decode_command`
   (used by tests and tools) keeps ignoring it. The cold-subscription decoder
   is unchanged, since subscriptions carry no reply.
3. **Engine.** A new `Engine::handle_command(now, command, timeout_ms)` stamps
   the command's absolute caller deadline, `now + timeout_ms`, keyed by its
   completion id, then dispatches `Event::Command` exactly as `handle` does
   today. `handle(now, Event::Command(..))` is unchanged and records no
   deadline, so native hosts and existing tests keep their behaviour.
   - The stamp survives deferred re-dispatch, such as held cancels released
     later, so a re-entry sees the remaining time, not a fresh wait.
   - It is removed when the command's terminal is emitted (the shared
     `finish_sealed_command` path).
     Storage is bounded by the number of commands awaiting a reply; a
     duplicate live id keeps its first stamp.
4. **API.** `Cx::command_timeout_ms() -> Option<u32>` is read-only and grants no
   send, completion or retry right.
   - It answers only inside the current command's genuine entry callback (the
     same custody as `admit_http_command`), and is `None` anywhere else (timer,
     socket, HTTP or other callbacks).
   - Inside the entry it returns the remaining whole milliseconds to the
     caller's deadline (at least 1), or `120_000` from receipt when the host
     sent none. `120_000` is the documented SDK default
     (`DEFAULT_COMMAND_TIMEOUT_MS`), the same value the host would have used.
   - A deadline already passed returns `Some(1)`: the venue's own admission
     then fails at once, under the normal timeout path.
5. **No other effect.** Nothing in the engine enforces this deadline by
   itself. Venues opt in by using it as a root `Policy.timeout_ms`. The host's
   own timer and its `COMMAND_TIMEOUT` result are unchanged.

## Consumers

Hyperliquid history (next slice) sets its original Read root to
`command_timeout_ms()` and mints each 20 s page child at queue dequeue. No
other venue changes here. The CALLERS entry distinguishes:

- **Caller abandonment:** the host timer rejects the promise, and the engine
  keeps any in-flight transport.
- **Engine-root expiry:** an adopting venue stops issuing work at the same
  deadline.

It also states the 120000 default for hosts that send no wait.

## Files

- `sdk-rs/engine/src/command.rs`: header field and the `HostInput::Command`
  shape.
- `sdk-rs/engine/src/engine.rs`: `handle_command` and the deadline stamp.
- `sdk-rs/engine/src/request_context_entry.rs`: `command_timeout_ms` beside
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

Engine, through real decode and dispatch:

1. An envelope with `timeoutMs: 300000` yields 300000 from
   `command_timeout_ms()` inside that command's `on_venue_action` or
   `on_command`. Without the field the answer is 120000. RED today, because
   the field is dropped (the control routes the value through a probe venue).
2. A held command released 40 s after receipt sees about 260000, not 300000.
3. `None` in timer, HTTP response and socket callbacks, and for a different
   command's entry.
4. Invalid `timeoutMs` (0, -1, 1.5, "1", 2147482648) is a decode error. An
   absent field and a present null are distinct; null is refused.
5. The stamp is gone after the terminal; a reused command id starts fresh.

Host:

6. `host.mjs` sends `timeoutMs` equal to the per-call override, else the
   configured default, for reply commands only.
7. Existing host deadline tests are unchanged.

## Gates

- Engine tests with and without features; WASM host JS tests; native host build.
- Strict Clippy and fmt on touched crates.
- Both request-context checks unchanged.
- Independent reviews: uufub4 (decode and entry custody) and 5c57jl (host),
  plus an adversarial pass.
- Architecture gate: No. A single, read-only engine fact the host already owns.
  It grants no authority, and no venue changes behaviour without adopting it.
