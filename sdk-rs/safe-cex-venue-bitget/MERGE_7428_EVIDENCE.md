# PR 7428 integration merge

Tested merge SHA: `5282556754832bc25b06a0c80de72a53a0a2db81`.
Parents: branch `31e338a93ab3b754c4386424880b8ca559bca41f` and requested
integration `355683b84a7c62f3f5c9e91910c7e15438c57076`.
The sole CLAUDE.md conflict retained both complete conflict sides; only the
three conflict marker lines were removed. No guidance/memory was re-read.
This evidence is committed separately; its documentation SHA was not gate-tested.

**The whole crate is not byte-unchanged from `965e254c80`.** The exact scoped
`git diff --exit-code 965e254c80 -- sdk-rs/safe-cex-venue-bitget` measured exit 1.
Integration added account_read/subscriptions source, tests, scripts, documentation
and two lib.rs module declarations. Shared exchange dependencies and Cargo.lock
also changed. The scoped diff on the original four-body source/test/capture paths
measured exit 0: socket_ping.rs, account_predicates.rs, tests/socket_policy.rs and
scripts/socket-policy-capture.cjs. The earlier gates cannot be treated as covering
this changed merged crate, so they were rerun.

At the merge SHA, measured sequential Cargo gates:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2`: exit 0, zero compiler errors.
- `CARGO_BUILD_JOBS=2 node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js venue-oracle:bitget`: exit 0; complete output read through Done, including source prechecks and all native/live tests. Environment included `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0, would update 0 manifests / 0 added / 0 orphaned.
- `git diff --check` before merge commit: exit 0.

This merge moves **0 new rows**; it resolves the open port PR's conflicts and
adopts integration's already-reviewed work. Original four row controls/review
remain bound to `965e254c8085a312e61e6d67851b16fd9d0d2d5a`; no new mutation pass
is claimed for this merge. No workspace suite/build, host or wasm Clippy/build,
CI rerun or real venue/socket calls were run.
