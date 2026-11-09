# Native TWAP restore ownership

Use `TwapFactory::with_owner` or `TwapBook::with_owner` with the authenticated
host's `TwapOwner { owner_id, generation }` to enable persisted restores.
The owner must come from the host session, independently of account names and
incoming saved rows. Dispose the old account scope and construct a new factory
when that owner or generation changes, or use `guard_owner` in a shared host
to drop retired schedules and ignore their deferred placement completions.

New TWAPs persist `ownerId` and `ownerGeneration` from that trusted snapshot.
A restore must match both tags and the account name. Foreign rows, stale
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

The native server binds the verified authenticated user and a generation
derived from that user, so the same user keeps it across same-socket reinit,
reconnects, signing handovers, account rebuilds, restarts and box moves: the
approved 60-second COSIGNED resume window keeps the account's TWAPs. Only an
authenticated owner change yields a different generation; queued TWAP ops
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
