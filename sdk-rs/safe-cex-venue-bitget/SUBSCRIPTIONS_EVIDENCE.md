# Pending subscription delegates

Moved **0 rows pending→ported, 2→divergent** in this slice:
`BitgetExchange.listenOHLCV` and `BitgetExchange.listenOrderBook`.
Including account_read, this branch moves **0 pending→ported, 3→divergent**
against integration `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.
Python 3.11 `tomllib` measured all eleven manifest files at that revision and
the tested SHA: 37 ported / 71 divergent / 119 pending → 37 / 74 / 116.
Both revisions have 227 rows with identical symbol/kind keys; exactly the three
named rows change status. This census is not proof of full Bitget parity.

**Tested code SHA:** `d9fec76a880c54940dae88e983123c65723d0839`.
**Actual parent:** `f2d527b1787d7d0e310b25c86b695feefe5899f5`.
That parent includes the separately reviewed account_read code
`4b7b60ff5fcc4cff68414c5243034d30556659a7` and its evidence-only commits;
these are branch ancestors, not a claim that they are merged into integration.
The original worktree parent was `e8091ad6f31e764c6ed96d85ba406d40146e2cbc`.
This evidence is committed separately; exits below cover the tested code SHA,
not a later documentation SHA. No source or manifest changed after these gates.

All Cargo invocations ran sequentially. Source tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`;
Cargo PATH included `/opt/homebrew/opt/rustup/bin`. Runtime: darwin arm64,
Node v26.7.0.

| Gate at tested SHA | Measured result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | exit 0; finished dev profile, zero compiler errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-bitget --test subscriptions --jobs 2 -- --include-ignored` after controls restored | exit 0; 2 passed, 0 failed, 0 ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | exit 0; complete output read: 7341 symbols, would update 0 manifests, 0 added, 0 orphaned |
| `rustfmt --edition 2024 --check` on the two new Rust files | exit 0 |
| `git diff --check` and restored `git diff --exit-code` | exit 0 / 0 |

The extractor's parse/serialize API appended file provenance; the writer ran
after manifest editing and reported zero reconciliation changes. No array was
hand-authored. No full venue oracle rerun, global manifest-checker rerun, workspace
build/suite, host Clippy, wasm Clippy/build, real sockets or CI rerun is claimed
at this SHA. Earlier venue/global checker results belong only to the account_read
SHA in [ACCOUNT_READ_EVIDENCE.md](ACCOUNT_READ_EVIDENCE.md), including its failed
global checker outside Bitget.

Each newly moved row has one compiling discrimination control, applied on disk
and restored byte-for-byte. Overwriting the forwarded OHLCV limit with `0.0`
fails `SUBSCRIPTION native limit` at exit 101. Appending `-wrong` to the forwarded
order-book symbol fails `SUBSCRIPTION native order book symbol` at exit 101.
Neither survived. The restored two-test target exits 0. These were two scoped
one-time controls, not an exhaustive mutation matrix.

Native assertions inspect all five decoded OHLCVOptions fields, including
negative-zero from bits, the forwarded symbol, both original Rc callbacks,
original Cleanup allocation pointers, deferred cleanup and callback/call counts.
Each decoded Candle/OrderBook field is asserted directly; one bid and one ask
are independently expected. The full native helper runs before source capture,
without serializing/re-encoding native records. The source supplement executes
the actual class fields, directly asserts options/callback/cleanup identity and
callback record identity, then emits effects. This is type-erasing transpilation,
not semantic TypeScript validation. Enumeration stops after one populated options
case and one callback record per delegate; arbitrary inputs/lifecycle are unproven.

Reviewer `/root/account_read_review` reviewed full tested SHA
`d9fec76a880c54940dae88e983123c65723d0839`: **no blocking findings** within the
declared shared-provider boundaries. It ran the actual-source capture (exit 0,
two calls, callbacks and cleanups) and read-only source/manifest/CLAUDE inspection.
It did not run Cargo, extractor, mutations, Clippy, wasm or CI. Both rows remain
CHOSEN divergent for owned typed options/records, single-use Cleanup and UTF-8
boundaries. Concrete socket construction and automatic Exchange registration
remain pending.
