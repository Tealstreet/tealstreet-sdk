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
