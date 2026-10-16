# WOO V3 listen-key and heartbeat slice evidence

Branch: feat/woo-v3-listen-heartbeat.
Fresh base: 3a4f19c35ab9a8a795bef393a969448fc2cad908 (3a4f19c35a).
Integration merged down: 18465853456056541848be3bb9e60ff3c68f565e (1846585345).
Measured source/test/manifest commit: **32cfec248bb0e33624776ef7772884113b2c0a13**; the integration merge is this commit.
This sheet and its JSON are added in a later evidence-only commit. The exits below were actually measured
at the source commit, not at that later documentation commit. No unrelated unreviewed lane branch was
used as an ancestor; our two source commits remain subject to the user's PR review.

## Gates actually executed

| Gate | Exit at 32cfec248bb0e33624776ef7772884113b2c0a13 |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package, all-targets, jobs, offline and locked scope | 0; observed 35 targets / 89 passed / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, 512 source files; Would update 0 manifests, 0 added, 0 orphaned |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows, 245 indexed public Rust items |
| rustfmt --edition 2024 --check on src/v3_ws_session.rs and tests/ws_session.rs | 0 |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-session-controls.py | 0; four required named failures and exact-byte restored passes |
| git diff --exit-code after controls | 0; source restored exactly before evidence was added |

All cargo commands ran serially. Tests/controls used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
The extractor output text was inspected; this claim does not rely solely on the yarn exit.
parity:extract was used as the manifest writer after editing mappings, and rust_files was derived from
real rs targets using tomllib. The scoped target audit reads 22 WOO/journal manifests and indexes only
the WOO package, with sibling crates symlinked for target resolution; it is not a workspace parity audit.

Initial diagnostics at 5dd5798f7b1f7b35d7781f7b3c238516e2a1f7ae: scoped cargo check exit 0;
scoped target audit exit 1 because it could not index an item macro. The macro was replaced by three
explicit Serialize/Deserialize/JsNumberOutput implementations in 80dd478bdf before the passing final gates.
The sub-agent review found missing independent source object-size assertions; body size 1 and heartbeat
size 4 are now pinned. The reviewer inspected source parity and fixes and ran the source oracle (0),
but did not execute Rust gates or mutations.

## Native assertion floors and discrimination

The oracle transpiles and executes the whole actual woo.v3.contracts.ts and woo.v3.websocket.ts modules,
exporting the actual private scalar helpers. No production helper is rewritten in the oracle.
Source number values are observed before JSON projection with explicit -0/infinity tags.
Rust assertions inspect native fields and numeric bits directly; no new test compares Rust re-encoded bytes.

Fixed independent walks stop at scalar input 19 (boolean, integer with minima 0/1, and ping), decimal input
25 (decimal, nonnegative, positive), listen response 16, pong frame 12, and URL case 8. Declaration tests
inspect four native record fixtures: distinct expiry/timestamp 37/11, retained -0/infinity, ping and pong
literal fields; 11 malformed record/literal fixtures are rejected. Request body and all four heartbeat
fields are asserted. These are finite floors, not complete runtime or mutation enumerations.
Multiply-invalid source fixtures verify first-error order: success before timestamp, timestamp before
data, expiry comparison before authKey, and pong cmd before success before time.
The source frozen-object observations are separate from native owned records.

| Family | Compiling production mutation | Named assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- | --- |
| Scalar validators | safe integer returns n + 1 | WS-SESSION native integer | 101 | 0 |
| Native declarations | symmetrically swap expiredTime/responseTimestamp codec names | WS-SESSION native model expiry | 101 | 0 |
| Listen session | returned expiry takes response timestamp | WS-SESSION native listen expiry | 101 | 0 |
| Heartbeat | ping builder emits PONG | WS-SESSION native ping cmd | 101 | 0 |

The four controls ran as explicit overlays on 32cfec248bb0e33624776ef7772884113b2c0a13, each compiled and failed its named native assertion.
Every restore passed all four ws_session tests. Stopped after mutation iteration 4, with zero survivors
in that floor; no additional matrix or per-symbol mutation coverage is claimed. Exact call sites, patches,
assertion excerpts, exits and commands are preserved in WS_SESSION_GATES.json.

## Independent row census

Command: python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-session-census.py 18465853456056541848be3bb9e60ff3c68f565e.
Python tomllib independently reads all 22 selected WOO/journal manifests, asserting the known 534-row
inventory and a separately enumerated 14-symbol expectation. The fresh 3a4f19c35a base was also compared
and has identical WOO counts to the merge parent.
**14 pending rows moved: 0 ported, 14 divergent.**
Before: 102 ported / 70 divergent / 362 pending. After: 102 / 84 / 348.
Every off-pending row has a real indexed Rust target and a canonical CHOSEN note.
The notes disclose Result strings versus Error identity, owned mutable versus frozen/Readonly records,
UTF-8/retained-JSON versus executable/prototype/accessor/lone-surrogate values, and widened native
String/bool constructor fields for ping/pong while declaration decoding enforces their literal values.
Safe integer operations retain -0 and source minimum rules. Decimal text is validated without numeric
conversion; private URL encoding retains encodeURIComponent's safe character set and UTF-8 escaping.
Declaration number fields are unrestricted f64; operation validators enforce the documented safe range.
These are pure session primitives: live socket/authentication/timer/lifecycle rows remain pending.
No HOW row movement is claimed.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for those.
