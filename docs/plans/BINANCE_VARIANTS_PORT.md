# Binance variants port

Branch: `feat/rust-binance-variants`, based on `origin/feat/rust-rewrite`.
Follow `VENUE_PORT_BRIEF.md`; all PRs target `feat/rust-rewrite`.

## Phases

1. Public profiles: extend the existing Binance paths table and options for
   Spot and COIN-M. Preserve USD-M/PM. Verify markets, ticker seeds/streams,
   spot versus futures depth continuity, candles and mainnet/testnet routes.
2. Private profiles: typed Spot signed WebSocket API subscriptions and account
   balances; COIN-M contract sizes, coin collateral valuation and private
   reconciliation. Preserve stale response and reconnect fences.
3. Trading and queries: profile-specific order types, identifiers, endpoints,
   split sizes, bracket handling, cancel/amend, unsupported command preflight,
   and history. Keep whole-batch client-ID validation before writes.
4. Integration and evidence: WASM/worker registration, strict independent replay
   tapes, public captures, CI coverage, roster/docs. Reproduce Hyro CPU misses,
   profile any genuine Rust loss and fix its cause without relaxing gates.
5. Final adversarial reviews: post findings on the PR, fix and repeat, rerun all
   required checks including USD-M/PM/Hyro. Merge only with green CI, then cut
   a fresh branch from the landing branch.

Every phase receives a sub-agent audit and commits at stable points. No stash,
rebase, force push or master changes. Official documented payloads and runtime
contracts define regression expectations. TS defects fixed deliberately require
`KNOWN_TS_FIXES`, commit-message and PR disclosure.

## Progress

- Planning: inspected the four TS Binance profiles and current Rust engine.
  Public/account implementation and Hyro investigation have bounded owners;
  trading/query/integration remains with the lane owner.


### Native implementation checkpoint

Public/private/trading phases share option selection and socket callbacks, so
these implementations are committed together after independent phase reviews.
USD-M/PM regression tests remain in the Binance crate. Native checks reached
73 passing cases before the final depth-generation regression; crate Clippy,
WASM-target Clippy, bans, root typecheck (44 tasks), root lint (28 tasks), the
first two actual WASM boundary cases and six independent frame tests passed.
Final rebuilt-artifact checks and replay evidence remain pending.

Phase reviews fixed protective lot splitting, ignored trigger intent, post-only
amendment semantics (including REST seeds), duplicated/reordered executions,
subscription ACK deadlines, Spot collateral valuation, native COIN-M sizing in
both worker/main frame appliers, and old depth snapshots crossing resubscription.
A real live Spot failure was isolated to large compressed subscription messages:
17,392-byte requests received close1008 `Payload too long` with Node's `ws`;
smaller messages worked. Public control messages now have bounded topics/bytes
and 400ms pacing; recapture will verify the correction.

Known TS defects deliberately fixed: retired aggregate Spot feeds; signature-only
Spot subscription liveness; split quantities rounded away; invalid/oversized Spot
protective legs; Spot fill price and replay/cancel identity; lost maker semantics
on amendment; COIN-M caller-ID preflight and duplicate/terminal execution state;
malformed inverse order snapshots. The harness checkpoint lists any further
verified differences. Defensive rejection and Rust-only bridge/depth fixes are
not presented as new TS defects.

Hyro unchanged baseline: three-run public PASS, TS/Rust main50.8/34.4ms,
steady17.6/13.2ms, retained16.1/0.9MB. Profiling found no confirmed production
regression. Machine-wide concurrent work prevents claiming strict isolation;
final fresh-artifact Hyro gates remain required with unchanged thresholds.


### Integration and final review in progress

- Fresh Spot capture after bounded subscription frames: 7,065 frames, 3,729
  markets and two books over 20 seconds; no policy closes. COIN-M live capture:
  203 frames/two books over 20 seconds. Both trimmed fixtures are being gated.
- First strict private replays pass for all four profiles: USD-M14 frames,
  PM6, Spot8, COIN-M6. Helper suite235 passes. These measurements precede the
  final lifecycle fixes and do not replace final-head gates.
- Final adversarial review found negative/tiny inverse quantities turning into
  full-position stops, cancellation acknowledgements accepted without proof,
  and amendments replacing already-filled quantities. Fixes and regression
  proofs are in progress before final gates/PR creation.
- TS retention audit: `registry.ts`, `registry.native.ts`, direct SDK creation
  and non-opt-in workers still instantiate these TS classes; only explicit
  `options.engine === 'rust'` selects the new registered profiles. Their TS is
  not provably unused. Deleting it would change the existing rollout policy and
  break direct SDK/default worker paths, so it remains the executable reference.


Initial public fixture gates (three-run medians; superseded by final-head checks):

| Profile | Frames | TS/Rust main CPU ms | TS/Rust steady CPU ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Spot | 7045 | 530.4/339.7 | 224.0/89.8 | 26.5/7.5 |
| COIN-M | 202 | 172.5/65.0 | 114.7/35.5 | 15.5/1.5 |
| USD-M | 873 | 189.8/89.6 | 98.8/34.6 | 18.9/2.6 |
| PM | 873 | 187.3/90.9 | 103.0/32.7 | 18.4/2.7 |

Root tests passed all27 tasks (6971 safe-cex cases in each suite). Harness
checkpoint `9a96deeaab` includes240 helper tests, strict raw signing and
subscription audits, truthful input-derived TS reference corrections, and both
live public fixtures. The final lifecycle checkpoint will rerun private gates
against authoritative cancel acknowledgements.

Nonblocking review limitation: native execution/terminal identity fences retain
per-order history for the account lifetime. This avoids forgetting stale native
identities; fixed-tape memory gates do not certify bounded lifetime retention.
No gate threshold or measurement interval was relaxed.


### Adversarial lifecycle checkpoint

Final native and independent review passed:96 native tests, strict crate Clippy,
240 harness cases, four real WASM boundary tests, root typecheck44 tasks,
lint28 tasks and root tests27 tasks (6971 safe-cex cases per run). The ten
original cancellation/amendment regression cases all fail on checkpoint
`23f35ebdf3`; the expanded seventeen-case lifecycle suite passes on the fix.

Cancellation now proves native identity, status and final cumulative quantity;
amendment renders only after that proof and subtracts final cumulative fills.
Filled wins over a late cancel. Unknown/malformed/partial rejection never retries
or advances replacement. Missing original rows, reused client IDs and concurrent
amendments through the replacement POST are fenced. Native terminal records also
protect later replica reads after cancel-all while retaining late executions.
Mixed Spot OCO replies are validated atomically. Raw inverse quantities that
would round into an unintended full stop reject before any batch action.

COIN-M public trades have a documented native regression. Unsupported private
history remains the existing TS empty-result contract; no unverified history API
expansion is claimed. Final replay rebuild is879753 bytes for Binance-only,
below the unchanged1258291-byte limit. Final exact-head gates remain in progress.
