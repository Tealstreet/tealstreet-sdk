# Automatic beta wasm release and pin

Change: Rust SDK calls `rust-beta-wasm-release.yml` immediately after its
`Rust / required coverage` job succeeds on a master push. The call depends only
on required coverage, so unfinished CPU gates and shared CPU certification do
not delay it. The guard independently requires completed, successful coverage
for that exact master SHA; the reusable entry point cannot skip certification.
The source must match the master push and pass the existing ancestry check.

The workflow-completion trigger remains a fallback. Both paths share release
concurrency and skip already-published input fingerprints, so a second trigger
for the same SHA succeeds without rebuilding or replacing its release. Automatic
cuts also skip source superseded by a newer published release. Manual dispatch
retains its existing `skip_certification: false` default and explicit override.
Publication still requires new `sdk-rs/` inputs. tealstreet-next
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

Verification:

```sh
node --test sdk-rs/harness/beta-wasm-release-workflow.test.mjs sdk-rs/harness/beta-wasm-release-guard.test.mjs
```

After a master push, the
`Rust / beta wasm release` job should start once required coverage is green,
even while CPU jobs remain pending. Missing or unsuccessful coverage still
refuses publication; `RUST_BETA_HOLD`, exact-tag custody and input deduplication
remain enforced. The later completion-triggered run should be a successful no-op.
