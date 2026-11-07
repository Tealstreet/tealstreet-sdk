# Targeted Rust CI history

Rust scope, selection, harness, bans, replay measurement and CPU jobs check out
the exact workflow SHA at depth one, then fetch complete history for that SHA,
immutable PR head/base commits, the base branch and master explicitly. No head
or tag wildcard is fetched. The checkout step remains separate for existing
physical-job and source-provenance authentication.

History is unshallowed before ancestry-dependent consumers run. The tested HEAD
cannot move. Immutable event-base and parent manifests, merge-tree validation,
first-parent landing/anchor checks and complete coverage remain required.
The scope, selection and bans jobs have ten minutes for legitimate ancestry
transfers rather than the previous five. No result, inventory or CPU gate is
relaxed. New history-helper changes select Rust CI through the shared scope list.

Real Git controls cover a PR composition, stale event base, master advancing
after checkout, a distinct base branch and malformed/missing immutable commits.
They pin full parent/merge/diff/inventory access while excluding unrelated
branch objects and both reachable and unrelated tags. No runtime or caller API,
Cargo.lock, dependency, WASM size or release automation changes are involved.
