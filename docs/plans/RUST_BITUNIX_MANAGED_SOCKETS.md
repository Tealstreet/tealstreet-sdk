# Bitunix managed-socket adoption

Owner: claude-4yjlw0. Assignment: claude-ddi2il (beta hardening, item 1). GO: claude-8x5tm1 (shared socket wiring and inventory). Review: claude-dazzy3. One PR into `master`: plan, originals, then source.

Base: `origin/master` at `5c8e70c1726`. Templates: Phemex (signed private login, `AuthMode::Required` with same-socket retry) and OKX (sharded public feeds). This supersedes the parked Bitunix+Bitrue plan; Bitrue is out of the beta scope.

## Why

`RUST_SOCKET_OKX_ONLY_VARIANT.md` left the Bitunix socket boundary legacy as a **remove-before-master obligation** (`RUST_MASTER_MERGE_BLOCKERS.md`). Done means Bitunix has no rows in `sdk-rs/scripts/socket-legacy-sites.json`, and the `bitunix` family has left `socket-legacy-families.json`.

The five legacy sites (`cx.connect` references):
- `lib.rs:351`: the venue `connect` helper (`cx.connect(Self::socket_spec(..))`).
- `lib.rs:908`: the futures feed connect.
- `lib.rs:924`: the public shard connects.
- `lib.rs:1136`: the supervisor reconnect.
- `account.rs:331`: the private feed connect.

## Change

Every Bitunix feed moves from the venue `Supervisor` and `cx.connect` to engine-managed sockets in one step. Once any feed registers, the engine admits only registered sockets.
- **APIs and hooks.** Feeds use `register_socket_feed`, `connect_socket`, `set_socket_interests`, `retire_socket`, `socket_epoch` and `send`/`send_signed`. The venue implements `socket_control`, `on_socket_event` and `encode_socket_interests`, with `SocketCapabilities { handoff: false }`.
- **Features.** Bitunix adds the engine feature `managed-sockets`.
- **Inventory.** The `bitunix` family and its 5 sites leave the socket inventories. The venue `connect` helper and `Supervisor` go with them.

| Feed | Auth | Traffic | Timings | Notes |
| --- | --- | --- | --- | --- |
| Futures (`ws-futures`) | None | AnyText | 10 s connect, 15 s ping, 45 s idle | `Invalidation::Required`: books clear and `book_changed` fires before retry. Book and candle interests are encoded as native `sub`/`unsub` frames. |
| Public shards (`{channel}:{start}`, one per 300 markets) | None | AnyText | as Futures | One feed per shard. Native 20-symbol batches go out at `subscription_interval_ms: 250`, the legacy cadence: 20-symbol chunks and a 250 ms queue timer (`lib.rs` on_open and on_timer before this PR). Retiring a shard does not clear books. Pinned by `public_shards_send_twenty_symbol_frames_250_ms_apart_and_again_on_reopen`. |
| Private (`/private/`) | Observe | TextAfterFirstPong | 10 s connect, 15 s ping, 45 s idle after the first pong | On `Ready`: `{"op":"ping"}`, then `send_signed(WsLogin)`. `socket_control`: `true` is Accepted, `false` is Rejected, and a missing, null or wrong-type result is Malformed. `recovery: RetrySameSocket { delay_ms: 2000 }` replaces the venue `login_retry`. |

The `recover` routing flag (true after a timeout, false after an ordinary close) comes from the retired epoch's reason and rides the reconnect's spec context, as today. The `WsLogin` recipe is unchanged.

## Behaviour kept exact

From `RUST_BITUNIX_SOCKET_LIFECYCLE.md` and `RUST_BITUNIX_LOGIN_CONTROLS.md`:
- URLs, feed identities and routing, and the `recover` context.
- Private bootstrap: ping, then login, then the balance, position and order re-reads when positions were already loaded.
- The 2 s same-socket login retry, coalesced and bound to its socket.
- A private socket is never idle-expired before its first real pong.
- Failures reset only on real traffic.
- Each open replays interests once.
- The uncertain-placement fence (#2283) and the census serial floor are untouched; sockets are not part of them.

The legacy suites stay green: `socket_lifecycle.rs`, `login_controls.rs`, `public.rs`, `routing.rs`, `order_rows.rs`, `trading_funnel.rs` and `closed_positions.rs`. Fixture edits only model a live stream (answering pings inside each idle window, following the latest socket); they change no assertions.

## Stricter than legacy (originals: `tests/managed_socket.rs`)

The managed lifecycle admits frames only on the open, current and unexpired epoch. These are production-binding: every test drives the real `BitunixFutures` through the engine.

REDs, failing on master `5c8e70c1726`:
- `futures_frame_at_idle_equality_is_not_published`: a futures frame at the idle deadline, before the watchdog timer is delivered, is not published.
- `private_frame_at_idle_equality_after_the_first_pong_is_not_reduced`: the same for the private feed after the first pong; the frame is not reduced.

Controls, passing on both trees:
- duplicate opens do not resubscribe or log in twice;
- frames before open are neither published nor reduced;
- each interest is subscribed once on open;
- the private ping precedes the login.

## Decisions for review

- **D1, private auth mode: `AuthMode::Observe` (changed from the plan's `Required`).** Observe opens straight to Ready: no login deadline and no gating of private frames, which is exactly the legacy session. Login evidence still drives `RetrySameSocket { delay_ms: 2000 }`, so the engine owns the retry. `Required` would add a 10 s login deadline and hold frames until acceptance, a tightening this PR does not claim. Each outcome:

  | Outcome | Engine | Venue | Legacy |
  | --- | --- | --- | --- |
  | Accepted (`result: true`) | cancels a pending retry | nothing | same |
  | Rejected (`result: false`) | one 2 s same-socket retry, latched | Error `AUTH_REJECTED` with the legacy text | same |
  | Malformed (missing, null or wrong type) | one 2 s same-socket retry, latched | Warn `STREAM_MALFORMED`, no auth claim | same |
  | No answer | no deadline; the socket stays current | nothing | same: legacy had no login deadline |
  | Close or retire mid-login | the retry is dropped with its epoch (`socket_driver` cancels `login_retry` on retire) | the replacement logs in on its own Ready | same: legacy dropped `login_retry` in `retire` |

  The pre-login ping is a plain `send` on a Ready epoch: under Observe, `opened` moves the feed to Ready, and `admit_checked` admits `Send` on Ready. The control `private_open_pings_then_logs_in` pins the order. Pinned by `login_controls.rs` (malformed, explicit refusal, latching, retired retry) and `private_login_has_no_deadline_and_frames_reduce_before_its_answer`.
- **D2, the first-pong gate.** `TrafficPolicy::TextAfterFirstPong` with a 45 s idle reproduces "silent until the first pong, then 45 s". It is pinned by `private_heartbeat_preserves_legacy_silent_sessions` and `private_first_pong_gate_preserves_silence_but_expires_from_actual_traffic`.
- **D3, books.** Every Bitunix depth frame is a full snapshot, so no snapshot barrier is registered; futures uses `Invalidation::Required`.

## Inventory and size

- Socket inventory: the `bitunix` family and its 5 site keys are removed. Request, signing and credential inventories are unchanged; the credential row is item 2 of the assignment, in its own PR.
- WASM: CI measures the Bitunix family. Any cap raise is named in the PR with the measured number and gets 8x5tm1's GO. The combined budget must hold.

## Order

1. This plan and the originals (REDs fail, controls pass), on `5c8e70c1726`.
2. On plan CLEAR and GO: the source, the inventory prunes, any fixture-only edits to legacy suites, and any budget line.
3. After SDK #2300 lands, merge `origin/master` for working Rust CI. Then review (dazzy3), exact-tree GO (8x5tm1), green CI, and the guarded merge.
