# Native TWAP restore ownership

Use `TwapFactory::with_owner` or `TwapBook::with_owner` with the authenticated
host's `TwapOwner { owner_id, generation }` to enable persisted restores.
The owner must come from the host session, independently of account names and
incoming saved rows. Dispose the old account scope and construct a new factory
when that owner or generation changes.

New TWAPs persist `ownerId` and `ownerGeneration` from that trusted snapshot.
A restore must match both tags and the account name. Foreign rows, stale
generations, and rows missing either tag are dropped without retaining timers
or placing slices. Matching rows preserve their existing schedule and sizing.

The unbound `new` constructors still support starting fresh TWAPs, but reject
all persisted restores. Hosts using those constructors must bind a trusted
owner before enabling restore; a TypeScript precheck cannot replace the native
ownership check. Existing rows without generation evidence are not adopted or
automatically assigned the current owner.
