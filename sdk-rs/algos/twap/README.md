# Native TWAP restore ownership

Use `TwapFactory::with_owner` or `TwapBook::with_owner` with the authenticated
host's `TwapOwner { owner_id, generation }` to enable persisted restores.
The owner must come from the host session, independently of account names and
incoming saved rows. Dispose the old account scope and construct a new factory
when that owner or generation changes, or use `guard_owner` in a shared host
to drop retired schedules and ignore their deferred placement completions.
Typed starts keep their captured binding through the book's owner refresh.
A replacement binding refuses a start discarded before dispatch; if a direct slice
or chaser was already dispatched, the typed result remains explicitly unknown.
`TwapFactory::start` returns the existing `VenueError` with `notSent` or `unknown`;
EXEC preserves that outcome in the correlated result instead of relabelling it.

New TWAPs persist `ownerId` and `ownerGeneration` from that trusted snapshot.
A restore must match both tags and the account name. Native identity-bound
`new_with_owner` constructors take the same full `TwapOwner`, including its
trusted generation, and permit a renamed label only for that verified identity.
The original persisted option/state labels must agree before remapping; the
owner/generation are never synthesized from the receiving book. Foreign rows, stale
generations, and rows missing either tag are dropped without retaining timers
or placing slices. Matching rows preserve their existing schedule and sizing.

The unbound `new` constructors still support starting fresh TWAPs, but reject
all persisted restores. Hosts using those constructors must bind a trusted
owner before enabling restore; a TypeScript precheck cannot replace the native
ownership check. Existing rows without generation evidence are not adopted or
automatically assigned the current owner.

The WASM host has one explicit compatibility exception: its existing
`algoStart(accountName, now, wall)` uses `with_legacy_restore` until the host
supplies an owner. This retains pre-release-8 account-name-only restore behavior,
including untagged rows; it never invents missing tags. The separate
`algoStartOwned(accountName, twapOwnerJson, now, wall)` requires both persisted
tags to match its trusted owner. Invalid or unavailable explicit ownership
never falls back to compatibility mode.

The native server binds the verified registry identity digest when present,
otherwise the authenticated user, with a generation derived from that binding.
Same-socket reinit, reconnects and same-owner signing handovers keep the current
generation: the approved 60-second COSIGNED resume window keeps the account's
TWAPs. Fresh bindings for the same identity use the same initial generation across
account rebuilds, restarts and box moves. Only an in-place authenticated owner
change advances the generation, and returning to an earlier owner never
reinstates its retired generation. Queued TWAP ops
captured before that change are refused, and remaining slices from the old
owner are dropped. EXEC captures the binding before a sleep
or wait and cannot start a TWAP after it changes. Already-issued venue writes are not undone;
existing slice chasers retain their normal lifecycle.

When ownership is bound, pre-release-8 untagged rows do not resume. The native
restore reports `TWAP <symbol> not restored: persisted TWAP owner or generation
does not match`; the server forwards it as an error with `forceToast: true`.
The TypeScript restore wrapper refuses a missing owner ID earlier with
`TWAP <symbol> restore cancelled-for-owner-change: persisted ownerId is missing`.
Neither path assigns missing ownership evidence from the current binding.

The native `actual_worker_round_trip_preserves_native_restore_ownership` test
runs the real TypeScript restore wrapper before restoring its JSON into the
native book. It requires Node and the workspace's Yarn dependencies. It checks
that matching rows resume with the same schedule and sizing, while stale or
missing generation evidence remains rejected through that boundary.
It is opt-in so Rust-only jobs need no JavaScript dependencies. From `sdk-rs/`,
run `cargo test --locked -p safe-cex-algo-twap -- --include-ignored` after installing
the workspace dependencies with Yarn.

# Shared TWAP outcomes and legacy parity

TWAP releases one scheduled child at a time. Quantity, price and notional rules
remain delegated to native writes. A definite refusal leaves later distinct
children scheduled, as in legacy TS. This includes a fully known native partial:
a placed prefix plus definitive refusal with no Unknown element. Its native
journal, IDs and refusal text are retained, even without an aggregate outcome.
Only actual uncertainty in the aggregate or native journal halts the action.
There is no new whole-plan admission gate.

Genuine Unknown halts the action under Sean/daxx's recorded parity exception
(2026-11-08, decision #3355; Sam to verify). It clears future child and fallback
timers and stops owned chasers while retaining their resting orders. It never
replays an uncertain child or issues a cancellation/replacement because of Unknown.
Previously dispatched responses still settle and update the reported native IDs.
The Unknown-only chaser halt also relinquishes orders whose placement response
arrives later. Ordinary timeout removal still cancels before market fallback
(legacy `TwapInstance.ts:213-220`); generic keep-order removal retains its legacy
late-response cancellation (`ChaserOrderManager.ts:307-322,1017-1035`).
Real child chasers expose an Unknown placement while read-only identity lookups
are still pending, so even a shorter slice interval cannot release another child.
An unresolved placement retains that signal in the ended chaser's progress.
Native journal uncertainty overrides an outer refusal even with accepted IDs.
A TWAP chaser's Unknown cancel immediately retains that progress signal and
relinquishes remaining automatic cancels; it never enters the cancel retry loop.
Standalone chaser cancellation keeps its existing retry policy.

The exact local wording is `The outcome is unknown. Check Open Orders`, matching
`engine/src/http_evidence.rs:38` per the lead's literal rule. Shared engine code
and the engine duplicate-write fence are unchanged by this follow-up.

The shared reporting-only WritePlan records indexed placed, applied, unknown and
not-sent rows. `twap:outcome` carries the structured error, native partial IDs and
all element outcomes; a `telemetry` event carries the same error with the kind
`twap.unknown_outcome`. The normal `error` op carries the canonical user message.
These are AccountPort emissions; the tests do not claim backend Sentry delivery.
Accepted chaser handoffs use `applied/deferredChild`, not fabricated native IDs.
Optional market remainder writes occupy separate journal rows. Completed parents
retain outstanding remainder responses and observe child chasers until settled.

Legacy source is pinned to `c78bed93d6eb4e45a3ec072b7d09fc1529d10a9b`.
All five paths are TS-partial and TS-continues after failure, including Unknown;
Rust retains TS continuation for definite refusals and records the explicit
stop-after-UNKNOWN exception for each path:

| Path | Legacy TS evidence | Unknown recording test in `tests/after_failure.rs` |
| --- | --- | --- |
| scheduled-TWAP-children (fresh schedules, every venue) | `packages/safe-cex/src/webworker/twap/TwapInstance.ts:249-280` | `unknown_exception_fresh_schedule_halts_with_outcomes_and_telemetry` |
| quantity/min/max/grid/notional/price delegation (including BingX unbounded maximum) | `packages/safe-cex/src/webworker/twap/TwapInstance.ts:117-139,167,255-280` | `unknown_exception_native_rule_delegation_halts_with_outcomes_and_telemetry` |
| restore/resume child preflight | `packages/safe-cex/src/webworker/handlers/TwapHandler.ts:95-99; packages/safe-cex/src/webworker/twap/TwapInstance.ts:291-310,330-345,255-280` | `unknown_exception_{restored,resumed}_schedule_halts_with_outcomes_and_telemetry; unknown_exception_restored_in_flight_halts_without_replay_or_suffix` |
| runtime rejected/unknown child continuation | `packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280` | `unknown_exception_runtime_reply_halts_with_outcomes_and_telemetry` |
| chaser handoff and compensating market remainder | `packages/safe-cex/src/webworker/twap/TwapInstance.ts:181-194,213-220,255-280` | `unknown_exception_market_remainder_halts_without_next_chaser; unknown_exception_chaser_halts_siblings_without_cancel_or_replacement` |

Each Unknown pin cites `PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx,
2026-11-08)` and its legacy TS file/line. The tests assert no subsequent action
writes, the indexed outcome and exact message, and the journal/telemetry emissions.
The recording AccountPort and chaser fixtures observe algorithm commands, not
physical venue HTTP. Their clock advances epoch time with Tokio's paused clock.

Definite-refusal controls retain the legacy sequence: fresh/restored/resumed
schedules release the third child after the second refuses; BingX's unbounded cap
remains usable; refused chaser handoffs allow later children; and a definite
market-remainder refusal retains chase -> cancellation -> market remainder ->
next chase. Compensation before an Unknown remains intact. Last-child Unknown
remainders, uncertain restored in-flight work, and late successful responses are
covered separately. Skipped zero slices retain their original journal indices.

KNOWN-ISSUE (legacy TS also partial-places): a valid prefix can land before a
later native-rule refusal. The earlier full-plan validator, notional sidecars and
dispatch-controlling helper adoption were reverted. The reporting helper cannot
veto dispatch; TWAP's explicit Unknown policy decides the action's stop.
