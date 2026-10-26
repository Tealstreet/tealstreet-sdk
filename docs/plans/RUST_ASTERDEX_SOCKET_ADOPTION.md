# AsterDEX managed-socket adoption

Owner: claude-19pt8s. Ruling: claude-ddi2il (interim shape B, below). One PR:
plan, originals and source.

## Change

AsterDEX's public stream and private user-data stream move from venue
supervisors to engine-managed sockets: `register_socket_feed`,
`connect_socket`, `set_socket_interests`, `retire_socket`, `socket_epoch` and
`send`, with the venue hooks `on_socket_event` and `encode_socket_interests`.
Both feeds move together, because once any feed registers, the engine admits
only registered sockets.

- Both legacy socket sites (`lib.rs`, `account.rs`) and the AsterDEX socket
  family leave the inventories.
- AsterDEX declares `SocketCapabilities { handoff: false }`. Nothing waits on a
  subscription release.
- Both feeds use `AuthMode::None`, `TrafficPolicy::AnyText` and
  `ControlPolicy::None`. The listen key in the private URL is the credential,
  so neither feed logs in.
- Timings are the legacy supervisor's: 20 s idle, a 5 s protocol ping
  (`LIST_SUBSCRIPTIONS`, id -1 public and 42 private), and a 10 s connect
  bound. Backoff starts at 500 ms (public) and 1 s (private).

## Behaviour kept exact

- The public open sends `LIST_SUBSCRIPTIONS` first. The engine then replays the
  desired topics (tickers, mark prices, books, candles) as one `SUBSCRIBE`
  frame. Subscribe ids keep increasing across reconnects.
- Commands change the desired set, and the engine encodes only the delta
  (one `SUBSCRIBE`/`UNSUBSCRIBE` per change, as before).
- A public retirement clears every book until a new partial snapshot arrives.
- A private open sends `LIST_SUBSCRIPTIONS` and runs the open-orders refresh,
  which is still the only reconnect resync. Balances and positions follow the
  poll.
- `listenKeyExpired` and a failed keepalive on the live key reconnect on a
  fresh key. A late answer for a retired key cannot touch the replacement.

## Stricter than legacy (pinned by REDs)

The managed lifecycle admits frames only on the open, current, unexpired
epoch. These REDs fail on the legacy supervisor:
- a duplicate open does not resubscribe (public) or refresh again (private);
- a frame before open is not published or reduced;
- a frame at the idle deadline, before the watchdog timer is delivered, is not
  published or reduced;
- a late pong cannot revive an expired private epoch; a reconnect follows.

Test fixtures were adjusted only to model a live stream, green on the legacy
tree with no assertion changes:
- `fire_after` answers pings inside each idle window;
- `socket()` follows the latest private socket;
- one case reconnects instead of faking a duplicate open.

## Interim listen-key acquisition (shape B)

`RUST_BACKGROUND_SESSION.md` reserves listen-key feeds for Phase C, where the
engine fetches the key before connecting. Phase C is not built, so by ruling
the venue owns acquisition until then:
- `Reconnect` triggers the signed listen-key POST, and its answer connects the
  feed. A 10 s acquisition deadline per create keeps the legacy watchdog over
  the POST.
- A failed or late create fences the key epoch and retries with the venue's
  own backoff: 1 s base, 30 s cap, jittered as the supervisor was. The failure
  count is reset on open and continues across socket retirements, so the
  legacy 1 s, 2 s, ... sequence is kept.
- The feed never returns an error for listen-key trouble, because an error
  would stop the feed permanently.

Without this, a failed create would leave the feed granted with no deadline
and the private stream silently dead. A control pins that a failed create
retries until the stream opens.

Migration: Phase C moves acquisition, its deadline and backoff, and the 30 min
keepalive into the engine's session-to-feed link. The venue then deletes its
`key_deadline`, `key_retry` and `key_failures`.

## One deliberate difference

`InterestsRefused` on the public feed raises a notice and keeps the feed
serving its other topics. Bybit stops the feed. A refusal here can only be
capacity or an invalid topic, and stopping would drop every live book.

## Size

CI decides. Linking `managed-sockets` is expected to cost about 35 to 50 KB,
inside the standing 5% venue allowance. The new cap is CI's number rounded up
to the KiB. Combined is measured once.
