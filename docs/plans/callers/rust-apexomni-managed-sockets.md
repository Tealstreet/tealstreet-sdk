# ApeX Omni managed socket custody

SDK, NX and server callers keep the existing public/private feed policy and
native financial wire bodies. The engine owns registered feeds, current epochs,
timeouts, heartbeat and reconnect backoff; ApeX declares managed_sockets().
The two raw connect sites and the ApeX legacy socket family are removed only
after production-binding controls and current replay/CI qualification pass.

Public supervision retains 30s connect, 30s idle, 10s ping and 1s..16s backoff.
Private retains 10s connect/auth, 30s idle, 5s ping and 1s..30s backoff. Both use
native text traffic for liveness. Only the current physical epoch may publish;
duplicate Opened, retired events and nonfinite deliveries are inert. Public
suppression must not suppress eligible private account work.

Public book/candle reference changes propose a complete projected Desired set
and commit the model only on synchronous admission. The engine's FIFO retains
pre-open and queued changes, duplicate references and reversals. Public Capacity
keeps committed demand, retires the current public epoch and invalidates books
before replacement publication; the private peer survives. Permanent errors
remain affected-feed fail-closed. Sequence repair uses genuine native interest
repair, retaining the exact unsubscribe/subscribe wire pair.

Each genuine private Opened signs the original login using installed Primary
SessionAuth, then sends the native three-channel subscription and ping before
the ACK, as the production proxy requires. Those frames belong to that epoch;
login is never an interest-replay frame. Required authentication accepts the
existing successful login or fully validated native account-data proof, retains
the independent 10s authentication deadline, and rejects failed login once.
Account reads and financial reducers keep their existing custody and ordering.

Orders reconciliation captures order_read_epoch once at read issuance. A clean
accepted snapshot publishes under the existing generation/revision guards before
orders_reconciled_since can clear uncertainty; a re-ask cannot freshen that token.
The required real-binding control marks uncertainty between old issuance and
publication, denies a retry after the old read, then permits it after a genuinely
fresh complete read. No other request-legacy or signing debt is cleared here.

Local originals precede source. Retain named REDs, unchanged working auth/wire
controls, exact old fixture assertions/payloads and current finite observation
clocks. Landing requires source review, scoped correctness/bans, current ApeX
family/replay evidence and 8x5tm1 exact-tree GO. No shared engine change, new
authority, raw fallback or synthetic Context/epoch is permitted.

First unchanged-runtime checkpoint: one working first-open subscription control
and four assertion REDs (declaration, duplicate public/private Opened, pre-open
ticker publication). The native private login/subscription/ping prefix is live
in the private duplicate-open control. Source qualification is still pending.

The candidate retains at most one private authentication proof, capped at 4 MiB.
Only strictly validated first account data can populate it; Authenticated takes
it before the native reducer runs at the actual callback clock. Retirement,
replacement or subsequent private classification clears the slot. An ordinary
ACK never populates it, and already-authenticated data follows the native path.
Malformed first data cannot publish a valid subset or restart the auth deadline.
The cap applies only to retained first-data proof, not authenticated row parsing.

Twelve production-binding controls pass locally, including exact-once proof
publication, revocation before Authenticated, old/new-epoch isolation, full-state
nonfinite ingress, duplicate queued demand, real 512-credit admission pressure
and encoded-frame Capacity recovery with a healthy private peer. Removing the
epoch check or invoking the proof reducer twice makes the corresponding control
assertion RED. The original first five bodies remain unchanged.

ApeX orders use one context per census. The existing lost-write/stale-read/fresh-
read control remains green with the captured first-issuance epoch. The core's
same-context response token already protects that case: removing this additional
epoch cap also remains green, so it is not claimed as a new runtime RED.

Fixture changes supply the genuine first Opened in one local decimal-stream
case and in the trading setup at its original catalogue observation. Trading's
delegated-write provider now signs SessionAuth inline alongside GET reads; write
signatures remain delegated. The stale-read control selects the exact emitted
native heal wake instead of the newly visible socket watchdog. All 269 existing
assertion invocations and 100 raw literals in those two files remain unchanged;
financial row bodies and their observation clocks are preserved.

On the initial composition, seven scoped targets pass: 141 tests and two existing
ignored remote-zkLink controls. This is local scoped evidence, not a full-family
or current-tip CI claim. Current composed strict/fmt/bans, source review and PR
family/replay qualification remain required before exact-tree auditor GO.
