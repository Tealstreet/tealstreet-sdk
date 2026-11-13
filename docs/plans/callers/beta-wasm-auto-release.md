# Automatic beta wasm release and pin

Change: `rust-beta-wasm-release.yml` publishes after every Rust SDK master push run
whose `Rust / required coverage` passed (CPU certification does not gate it) and
whose `sdk-rs/` inputs have no release yet. tealstreet-next
`rust-beta-wasm-pin.yml` lands the matching pin by PR once its CI passes. This
is a release-lane change: an SDK master merge now reaches the NX beta bundle
without a hand-dispatched release.

Callers affected: NX (the web prebuild pin, rust-preview and rust-prod).

Obligations, in order:

1. To stop a merged change reaching beta, set the repository variable
   `RUST_BETA_HOLD` to the reason in both tealstreet-sdk and tealstreet-next.
   The SDK release and the NX pin job then fail naming it. Clear both to resume.
2. The NX pin job uses `RUST_BETA_PIN_TOKEN` in the `rust-beta-pin` environment
   (branch policy: master only): a fine-grained token with contents and pull
   requests write on tealstreet-next only.
3. rust-preview and rust-prod take master only when the pin test is green
   (bundle-cut lane).

Deletion prerequisites: none added or cleared.

Known gap: a master commit whose Rust run was cancelled because a newer push
superseded it gets no release of its own. If Copybara imports exactly that
commit, NX has no matching pin and the prebuild refuses until the next import
of a released commit. To recover sooner, re-run the cancelled master push run
(`gh run rerun <id>`; it stays a push run, so `certified` accepts it once
required coverage passes), then dispatch `rust-beta-wasm-release.yml` with that
commit and pin the result. A plain dispatch alone is refused as uncertified.
The rerun is attempt 2, so its CPU measurement jobs fail by design (cpu-ci
requires the original attempt 1) and the run concludes failure. That is
expected: no CPU result is produced or trusted from attempt 2, `certified`
accepts the run on required coverage alone, and the CPU red is not a reason to
rerun again.
