# Account identity read

Moved **0 rows pending→ported, 1→divergent**: `BitgetExchange.getAccount`.
Python 3.11 `tomllib` measured all eleven Bitget manifests against integration
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`: 37 ported / 71 divergent /
119 pending → 37 / 72 / 118, both 227 rows. This is a manifest census, not
whole-venue behavioral proof.

**Tested code SHA:** `4b7b60ff5fcc4cff68414c5243034d30556659a7`.
**Actual parent:** `e8091ad6f31e764c6ed96d85ba406d40146e2cbc`.
`git merge-base --is-ancestor` confirmed that parent is contained in the observed
origin integration snapshot `dc69e81e1153b0724bafc7977bbba877a8e128fc` (exit 0).
No additional unreviewed branch ancestor was detected by that ancestry check.
This evidence file is committed separately; gate exits below cover the code SHA,
not a later documentation commit or later port.

## Measured gates

All Cargo commands ran sequentially. Environment: darwin arm64, Node v26.7.0;
live source commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo used `/opt/homebrew/opt/rustup/bin` on PATH.

| Command at tested SHA | Measured exit and output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0; finished dev profile, zero compiler errors |
| `python3 sdk-rs/safe-cex-venue-bitget/scripts/account-controls.py` | 0; 14 compiling mutations each exit 101; restored account target exit 0, 2 passed / 0 ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output says 7341 symbols, would update 0 manifests, 0 added, 0 orphaned |
| `CARGO_BUILD_JOBS=2 node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js venue-oracle:bitget` | 0; read complete output through `Done`; observed 58 passed / 0 failed / 0 ignored across 18 Rust test binaries; all five script prechecks passed |
| `rustfmt --edition 2024 --check` on `src/account_read.rs` and `tests/account_read.rs` | 0; only those two Rust files checked |
| `git diff --check` and restored `git diff --exit-code` | 0 / 0 |
| `cargo run --manifest-path sdk-rs/Cargo.toml -p safe-cex-parity --bin parity-check --jobs 2 --` | **1**; 19 status and 3 invention diagnostics outside Bitget, zero Bitget diagnostics |

The extra global manifest checker names AsterDEX rename/invention rows, two WOO
rename rows and `ExchangeError::js_error_string` without an exemption. Its named
manifest/source paths have zero diff against the actual parent (checked exit 0).
This is a failed gate, not a global parity pass. The required extractor and scoped
compile gates are green. No full workspace compilation/suites, host Clippy,
wasm Clippy/build, live venue calls or CI reruns were performed.

## Behavioral floor and discrimination

One actual-source TypeScript compilation/capture supplies 13³ = 2197 field cases,
14 outer/inner body shapes, two source read failures and three surrogate witnesses.
The independently fixed native field walk expects 1728 successful records and
469 coercion errors. Every native user/sub/affiliate field, presence distinction,
request and error assertion completes before any native output serialization.
The native walk stops at `(overflow, overflow, overflow)`, then inner/object,
then rejected read; surrogate witnesses stop at inviterId. These are finite
coverage floors, not exhaustive JavaScript or JSON enumeration.

| Restored mutation | Named failing assertion; exit 101 |
| --- | --- |
| parent/user selection swap | `ACCOUNT native user identity 13` |
| sub/affiliate replacement | `ACCOUNT native sub identity 6` |
| affiliate/sub replacement | `ACCOUNT native affiliate identity 6` |
| omit present-null subId | `ACCOUNT native sub identity 0` |
| null/undefined error replacement | `ACCOUNT native shape error 2198` |
| endpoint replacement | `ACCOUNT native request 0` (also pending endpoint) |
| recreate identical read error | `ACCOUNT original read identity` |
| skip awaiting read | `ACCOUNT read remains pending` (also native affiliate identity) |
| flatten read error | `ACCOUNT native read metadata` |
| empty source | `ACCOUNT required native input` |
| subset source | `ACCOUNT required native input` |
| extra source | `ACCOUNT source consumed count` |
| replace actual input value | `ACCOUNT input content 0` |
| same-length source replacement | `ACCOUNT input content 1` |

No mutation survived. Every edit was restored byte-for-byte and the restored
account target passed. Source empty/subset/extra controls have no source-total
guard to remove: the fixed required-input walk and consumed-count assertion carry
that proof. Native output is one account record or one error, so native collection
empty/subset/extra controls are **INAPPLICABLE**, not missing collection proof.

## Sub-agent review and limits

Reviewer `/root/account_read_review` reviewed full SHA
`4b7b60ff5fcc4cff68414c5243034d30556659a7`: **no blocking findings** within the
declared parsed JSON/UTF-8 boundary. It ran source capture and a separate read-only
capture checker (both exit 0), independently checking 2213 cases, endpoints and
three fixed surrogate fields. It inspected committed blobs during mutations;
it did not run or certify Cargo, extractor, mutation, Clippy, wasm or CI results.

Both source synchronous-throw and rejection witnesses map to Rust `ready(Err)`:
they establish original `Result` error forwarding, not Rust panic behavior.
Dropping a Rust caller drops its read future; this is native future ownership,
not proof of JavaScript Promise cancellation. Executable getters, callable fields,
custom prototypes and UTF-16 lone surrogates remain explicit CHOSEN representation
boundaries. Actual REST client and automatic Exchange registration remain pending.
