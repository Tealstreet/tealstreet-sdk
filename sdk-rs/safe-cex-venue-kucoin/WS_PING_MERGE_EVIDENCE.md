# PR 7432 merge repair

Fresh tested merge SHA: `c093d7a15aa10becf4a778813aa92e6706240f49`.
Parents: ping delivery `64adae869a642f0a01a73502426f2027a5469856` and
requested integration `51156f6fbdd0ba338683dc61f3b92bad7441cb31`.
This repair moves zero additional rows and retains the four ported ping rows.

Two additive conflicts were resolved by preserving both sides: lib.rs exports
both ws_ping and the integrated position_metadata module; venue documentation
retains both sections. No guidance was re-read to resolve those additions.

An independently measured scoped git diff --exit-code against `0ab77727b9`
exited 0 for src/ws_ping.rs, scripts/ws-ping.cjs, tests/support/ws_ping.rs,
tests/ws_helpers.rs and all four websocket manifests. Those reviewed ping
files are byte-unchanged. The integrated metadata source/module/mapping additions
remain intact; this is not a claim that the entire crate matches the old gate SHA.

Fresh completed gates at the named merge SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2`: exit 0, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test ws_helpers --test position_metadata`: exit 0, 8 websocket tests and 3 metadata tests passed, zero failed/ignored.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0; output inspected, 7341 symbols / 512 files, zero changed manifests, added or orphaned symbols.

Not run on this merge: mutation controls, full venue suite, workspace build,
host Clippy, wasm build/Clippy, Rust parity tests, or CI. Earlier controls remain
attached to their original gate SHA. The following documentation-only delivery
commit does not change the tested ping files or the resolved module exports.
