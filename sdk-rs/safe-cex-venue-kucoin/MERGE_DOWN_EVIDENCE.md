# PR 7397 integration merge repair

Fresh tested merge SHA: `94375aeaf7d099a09f2fb6a5f8c313dfb046ed5b`.
Actual parents: branch `fd9174455aa88e9a77a26b188f2d0ad5c3fc0af4` and
fetched integration `eb0041f3fed79ba218ed050bdbde6c36a9e45cff`.
`git merge --no-edit origin/feat/safe-cex-rust` exited 0 with no conflicts.
No new pending row moved in this repair; it retains the cancellation slice's
one pending-to-divergent move. Repairing the user-reported PR conflict is the scope.

`git merge-base --all HEAD origin/feat/safe-cex-rust` now returns exactly the
integration parent above. The diff against that integration parent has nine
KuCoin-only files at the tested merge, rather than the earlier 84-file diff.
This evidence-only file subsequently adds one more KuCoin documentation file.

`git diff --exit-code 2cf0ffc5e5 HEAD --
sdk-rs/safe-cex-venue-kucoin/src sdk-rs/safe-cex-venue-kucoin/tests
sdk-rs/safe-cex-venue-kucoin/scripts sdk-rs/parity/manifest/exchanges/kucoin`
was measured independently with exit 0. A second explicit subprocess check
including KuCoin Cargo.toml also exited 0. Reviewed code and all twelve
manifests are byte-unchanged. The prior sequential-cancellation evidence is
historical evidence, not the gate sheet for this merge.

Fresh gates completed at the named merge SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2`: exit 0, zero compile errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test spot_cancel --test notification_reads`: exit 0, 13 cancellation tests and 1 reporting test passed, zero failed/ignored.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0; output inspected, 7341 symbols / 512 source files, zero changed manifests, zero added/orphaned symbols.

Not run on this merge: complete venue suite, workspace compilation, host
Clippy, wasm compilation/Clippy, Rust parity tests, mutation control, or CI.
The code did not change, so the earlier mutation was not repeated or relabeled
as new-head proof. The evidence-only delivery commit does not alter the tested code.
