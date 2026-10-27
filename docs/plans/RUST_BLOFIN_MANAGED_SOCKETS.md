# BloFin managed sockets and order census epoch

One venue-only slice from master 5065bbf9b4d, after BingX #2304. Both feeds declare
managed_sockets and register before either connects; no mixed raw lifecycle.
No engine, signer, registry, host or checker changes. Every merge needs an exact
8x5tm1 GO on a fresh attempt-1 run containing master3dae492.

Public retains connect 20 s, idle 20 s, ping 5 s and backoff cap 16 s. Private retains
connect 10 s, authentication 10 s, idle 20 s, ping 5 s and default 30 s backoff cap.
AnyText refreshes liveness; pongs never prove authentication. Public AuthNone,
private AuthRequired, native controls and Reconnect recovery fit existing APIs.
The unchanged installed-Primary SessionAuth recipe signs one fresh login at each
genuine Opened. Native ping follows. Only definitive code 0 login ACK opens the
four private channel interests and account refresh; duplicate/stale ACK/open and
old signing completions cannot replay them. Rejected or malformed proof retires
only private; permanent pre-start key refusal retains native startup failure.
No signed login is retained as a replayable interest.

Canonical public desired interests cover tickers/funding for every market,
refcounted books/books5 and candle/books5 union. Encode the existing subscribe
and unsubscribe JSON in bounded 4096-byte batches. Provisional additions/removals
commit local demand only with the exact proposal; synchronous refusal retains
committed maps. Asynchronous Capacity retains desired work, invalidates public
books and uses shared Closed/backoff, preserving private. Permanent encoding or
invalid-interest defects and inherited initial Ready replay errors fail closed
on the affected feed. Sequence-gap book resync uses genuine repair interests,
never raw frames or new socket authority. No HTTP book handoff is needed.

Retirement clears sequence/book/resync before replacement rows and publishes
book invalidation. Engine owns connect, auth, liveness, ping and retry identities;
remove both legacy supervisors. Suppressed public must not block real private.
Preserve native URLs/testnet, provider wire, financial reducers and HTTP hooks.

The full order census spans three families and pages. Capture order_read_epoch
ONCE when the first page is issued and retain that minimum through every
continuation. Only clean current Primary census publication calls
orders_reconciled_since(None, captured_epoch). Keep orders_revision and the
uncertainty_generation guard: reasks join actual busy/retry work and never move
the census origin. A first-page-before-mark/last-page-after-mark regression must
remain fenced until an entirely fresh clean census.

Controls use real Blofin binding, installed provider, actual socket/timer IDs and
Prepare/Bind/Claim. First pin duplicate public/private Opened and required ACK,
plus explicit declaration. Extend with genuine reconnect/fresh signature, stale
and nonfinite full Store/action checks, sibling isolation, public suppression,
refcount/books5 union, sequence repair, real valid-key pressure and retained
recovery. Preserve old financial/assertion/payload bodies; fixture observation
changes are explicitly documented. Remove only BloFin's three fingerprint rows
(four occurrences) and family after the actual scanner proves zero raw sites.
No request/signing/credential inventory changes. Local scoped tests/strict/fmt/
bans, reviewer CLEAR, fresh correctness/CI family/replay and auditor GO gate it.
No local family matrix or combined build under the venue speed policy.

Qualification corrections and fixture contract:

- Public reconnect installs its replacement epoch before fallible interest
  admission. Real proposal-credit exhaustion retires that epoch through Closed,
  emits no connect/frame, keeps private alive and recovers desired subscriptions.
  The original pre-epoch ordering failed the captured 512-credit control. JSON
  argument validation uses RawValue syntax validation, preserving escaped native
  arguments without a JSON tree or gate exception.
- Financial fixtures explicitly open and acknowledge private only at their local
  setup sites. Constructors/seed/ws do not implicitly open. Three test wrappers
  forward the actual managed socket API; the login timestamp fixture starts at
  its original 1700000000123.875 observation. Public candle liveness uses genuine
  pongs without altering financial payloads.
- Two account terminal-memory/regressive-snapshot fixtures replace a duplicate
  login ACK reread trigger with real Close/captured backoff/newConnect/Open,
  retaining their original ACK and financial rows at the new observation.
  Default replies stay at current observation; explicit respond_at is unchanged.
  Trading pause controls select their real 350/1000 ms delay from the actual
  response observation, rather than an obsolete absolute timestamp.
- Existing 676 assertion invocations and 342 raw payload literals in the ten adapted
  fixture files remain token-exact/in order. Original 224 PASS / 78 FAIL and later
  fixture/setup failures remain separate from candidate qualification.

Local qualification of source correction 57521d79208: 316 PASS across 24 nonempty
native targets, including 18 lifecycle controls, 0 FAIL/ignored/filtered; strict
all-target clippy, package fmt and full bans pass. This is local evidence only. Publication, current-tip composition, fresh
PR correctness/family/replay and exact-tree auditor GO remain required.

CodeRabbit hold corrections are tested against the prior cb2adf6 runtime first.
The three reported cases and pending add/remove reversals fail there. A FIFO of
changes retains in-flight duplicate book and candle references, recomputes after
the exact commit, and stages demand against a current connecting epoch. Private
refusals cannot clear public pending state. Engine coalescing may avoid a net
no-op wire pair; controls assert the resulting live subscription set. The now
write-only connection-open field is removed. Financial reducers and all prior
financial fixtures remain unchanged. Caller obligations move verbatim into
callers/rust-blofin-managed-sockets.md when composing master under #2316.
