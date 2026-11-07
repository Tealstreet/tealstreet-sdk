# Caller obligations, one file per change

Since 2026-10-27, new SDK/NX caller obligations go here, not in
[RUST_CUTOVER_CALLERS.md](../RUST_CUTOVER_CALLERS.md). Concurrent PRs all
appended to that one file, so every landing conflicted every open PR, and each
re-merge cost a push and a full Rust run.

- Add one file per change, named after its branch without the prefix:
  `fix/rust-bitunix-managed-sockets` becomes `rust-bitunix-managed-sockets.md`.
- Edit only your own file. To amend an obligation another change recorded,
  add your file and name the file it supersedes.
- There is no index: this directory is the index. Do not add one, since it
  would collide the same way.
- `RUST_CUTOVER_CALLERS.md` keeps the obligations recorded before this date.
  Edit one of its sections only when that obligation itself changes.

Each file states the change, the callers affected (SDK, NX or server), the
obligation and its order, and the deletion prerequisites it adds or clears.
