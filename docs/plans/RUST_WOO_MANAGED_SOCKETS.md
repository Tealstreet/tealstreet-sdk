# WOO managed socket adoption

Baseline: actual `0768b2c4253a565320d3e8bc62606163ebf39f4e`, containing
managed-socket landing #1771. This is one venue's adoption; Bitunix stays
excluded under Sam's option-1 decision.

## Scope and ownership

The request-side owner, codex-ih35js, has no active WOO socket-region claim.
The snapshot owner, codex-mxb4mq, cleared the narrow socket regions against
#1788 at `a17186394c4c6f8f5753925d7a1cc91760aed594`. Preserve that PR's
order-token capture/clear/validation and native snapshot publication verbatim;
integrate its actual landing before final WOO CI if it lands first. Any source
conflict stops for maintainer resolution.

Production scope is WOO Cargo.toml, lib.rs socket plumbing, account.rs private
socket plumbing, and a small native socket_codec.rs. Add dedicated managed
socket tests; preserve existing wire assertions and native account/trading
reducers, HTTP builders, signatures, identities and freshness fences. Prune only
WOO from the socket-site/family inventories. No engine, host, workflow, build
profile, gate threshold or request-inventory growth is planned.

The request burn-down's nine SOCKET fingerprints are all excluded Bitunix. This
adoption instead removes WOO's two physical connect fingerprints/two occurrences
from the separate landed socket inventory. It does not claim HTTP conversion.

## Native contract

WOO has one public stream and one private stream, each with the same native
mainnet/testnet URL and application-id suffix. Public feed suppression remains
exact: Shared and None do not register or connect the public feed or arm a
public retry. Private startup remains independent of public suppression.

Both feeds preserve connect timeout 10s, idle timeout 20s, ping interval 6s,
base backoff 1s and stable threshold 30s. Public retry caps at 16s; private caps
at 30s and requires auth within 10s. Any current text refreshes native idle
liveness; pong still supplies the existing public latency measurement. A text
arriving at/after an expired boundary cannot renew that epoch first.

Explicit private auth refusal keeps the native reconnect/backoff policy. It
must not become terminal startup rejection or stop the public sibling. Duplicate
auth success neither repeats subscriptions nor refreshes the immutable auth
window. Missing/malformed auth proof cannot authenticate. Existing account
snapshots, polling and protected state survive private retirement.

Public open sends the same ping followed by tickers/bbos/markprices and native
book/candle/trade interests. Private open uses the unchanged websocket_auth
codec and account-refresh call; successful auth releases executionreport and
algoexecutionreportv2 subscriptions followed by the native ping. Native topic
JSON field order and ping/pong/auth bytes remain exact. No key/provider policy
change accompanies socket ownership.

## Shared-core mapping

- Enable managed-sockets only in WOO's engine dependency. Register two stable
  FeedIds lazily; keep only native connection presentation fields needed by the
  existing decoder. The engine alone owns current epochs, auth, timers and retry.
- Public policy: AuthMode::None, TrafficPolicy::AnyText, ControlPolicy::None,
  required book invalidation, zero subscription pacing. This preserves the
  cached-index ticker decoder without an additional public control JSON parse.
- Private policy: AuthMode::Required, AuthRecovery::Reconnect, no permanent
  refusal group, TrafficPolicy::AnyText, native control classification and no
  account-Store invalidation. Preserve the existing rejection notice text.
- The native control decoder recognizes private auth Accepted/Rejected/Malformed
  and pong. It supplies evidence only, never changes epochs or authentication.
  All private auth outcomes flow through engine lifecycle callbacks.
- Build complete desired interest sets from existing public subscriptions and
  the two fixed private topics. Use engine replacement/replay and one native
  frame per interest; preserve fixed-topic order and native subscription JSON.
  Reference counts remain venue facts. Duplicate opens cannot replay interests.
- Opened/Ready/Authenticated/Ping/Reconnect/Retired callbacks retain native wire
  construction and seed scheduling. Use send_after_socket_interests for the
  private post-auth ping so it follows subscription release. No local Supervisor,
  auth timer or reconnect counter remains. Propagate admission/encoding failures;
  do not silently accept partial coverage or add a venue fallback sender.
- Public retirement performs the existing per-book generation advance, buffer
  reset, book clear and book_changed notification before retry. Private retirement
  clears only connection presentation; native account HTTP/reconciliation state
  is untouched. Resubscription still starts native book HTTP seeds and buffering.
- Dynamic book/candle/trade subscription commands submit the resulting complete
  desired set once. Retain the current HTTP book-retry path and all command
  terminal ownership. Verify per-socket wire ordering and seed/buffer behavior;
  do not infer cross-transport ordering preservation without an executable check.

## Commit and verification gates

1. Commit this plan before tests or production changes. Audit API names, policy
   constants, field uses, queue limits and owner boundaries against the baseline.
2. Add assertion-only original RED controls using the real Engine<Woo>, without
   enabling the feature or editing production: duplicate public/private open
   cannot repeat wire output; expiry-before-late public data; expiry-before-late
   private auth. Retain positive controls for ordinary data/auth and record exact
   failures. Run existing WOO supervision/liveness/public/account controls as
   compatibility evidence; do not weaken their assertions or deadlines.
3. Adopt the existing core and remove the emptied legacy socket ownership. Add
   native codec/coverage, reconnect replay, suppression, malformed auth, retired
   events, private rejection backoff and public invalidation controls. Existing
   known protocol/error behavior remains explicit in the caller section.
4. Run focused WOO and engine tests, strict Clippy, formatting, bans and all
   shrink-only ratchets. Verify every other socket map/family and the full
   request manifest are identical, or record a narrowly authorized rehash.
   Audit the source diff and complete ownership/ordering paths after tests.
5. Publish a venue-only draft PR to feat/rust-rewrite with CALLERS/QUALITY/AUDITS
   sections and current evidence. Fresh full CI must pass public steady CPU,
   retained memory, module size and replay gates without profile/threshold changes.
   A timing miss uses only the existing one-rerun rule; a repeat is a real blocker.
   Green replay declarations do not establish private parity.
6. Integrate actual preceding train changes, resolve only standing-authorized
   independent docs appends, and take fresh CI after shared-source drift. Request
   a venue window; separate fresh scope/unowned/revert guard then head-matched
   merge only after explicit GO. Report actual parents/tree and remaining socket
   inventory. Continue with one separately coordinated venue per PR.

## Original controls and fixture custody

Checkpoint bc5415d7c09 has four assertion REDs and two positive live-wire controls;
its compile succeeds and each failure names a lifecycle boundary above. The
separate baseline compatibility run passes 194 tests. Source adoption makes all
six controls pass without changing their assertions.

Some financial account fixtures inject private data without any prior open.
Use an established-stream helper only for those native reducer fixtures; keep
bare-start startup/auth/watchdog tests. Anchor the fractional signing fixture's
connection clock to its intended fractional open. Preserve every existing
financial/wire assertion. One rejected-snapshot case opens before its first
private data and retains that open's queued refresh reads, preserving causal
order rather than treating a duplicate open as a reconnect. The #1788 owner
cleared this bounded fixture repair and requires actual reconnect transitions
when composing its later token controls.

## Independent ordering and fixture review

The independent paired baseline/candidate probe passes both dynamic/reconnect
subscription-before-HTTP controls on original 0768 and fails both on 27bc. Keep
combined-action assertions, rather than filtering socket frames, as mandatory
adoption gates. WOO waits for the shared socket release-order correction; no
venue-local deferred queue or manual subscription sender replaces that boundary.

The rejected-algo snapshot classification fixture must start its read after the
known native classification, so a newer-WS fence cannot mask metadata staging.
Establish and load the first stream, admit the classification, then use a real
close/reconnect/new open to start the tested snapshot. Keep every original
financial assertion and shift response times relative to the actual reconnect.

## Scratch refresh before the snapshot-owner landing

Plain-merged requested actual `77f12408947` into parked `76e868b8846` on local
branch `feat/rust-woo-socket-preintegration`; merge `2e238aa94ed`. Venue source
merged without conflict. The four shared docs retain current content plus this
adapter's suffix once. The socket-family conflict preserves the current complete
foreign map and removes only WOO; the sites merge removes only the original two
WOO fingerprints/occurrences. No request inventory or shared-core edit.

Scoped `managed_sockets` executes nine PASS and the same two combined-action
ordering REDs, zero ignored. The first command used the singular target name and
ran no tests; the recorded execution uses the actual plural target. Current
#1943 is available but this refresh intentionally preserves the parked adapter's
source; it is not handoff adoption or merge readiness. The dynamic/reconnect seed
must still use the actual shared handoff before final certification. The normal
socket checker and own-delta whitespace check pass. Whole-integration whitespace
also reports two unchanged incoming doc lines; their bytes were preserved.

Read-only trial against #1788 frozen `3943134fc60` finds one source conflict in
account.rs imports: retain the managed imports plus OrderSnapshotToken, without
restoring retired legacy Supervisor/Config/Tick/SocketChannel imports. All token
production hunks and appended tests merge automatically; owner review of that
exact resolution is requested. This trial is not actual #1788 integration or a
runtime claim for its appended cases. Wait for the actual snapshot-owner landing
and then recheck that composition without rewriting either frozen head.
