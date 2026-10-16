# Bitget account and position reads

Direct build parent: `d47e29496d501fa64d798557bf6e2e41902c884a` (integration).
Fresh branch: `feat/rust-bitget-account-positions`. No separate unreviewed lane
branch was used as an ancestor. Reviewed/tested code:
`874b05a537e58f183181410f1dcc0dc099314218`.

## Rows moved and scope

Python **3.11 tomllib**, comparing `(manifest, kind, ts)` identities with the
actual parent, measured **8 pending -> ported, 0 -> divergent**. Eleven manifests,
227 identical identities: **63 ported / 80 divergent / 84 pending** before;
**71 / 80 / 76** after. This is a manifest census, not exhaustive input proof.
The enumeration stopped at the eleventh sorted manifest,
`bitgetusdc.exchange.ts.toml`.

The eight targets in `src/account_positions.rs` are:

| TypeScript symbol | Rust body |
| --- | --- |
| BitgetExchange.fetchBalance | fetch_balance |
| BitgetExchange#fetchUtaBalance | fetch_uta_balance |
| BitgetExchange.fetchPositionMetadata | fetch_position_metadata |
| BitgetExchange#fetchUtaPositionMetadata | fetch_uta_position_metadata |
| BitgetExchange.fetchPositions | fetch_positions |
| BitgetExchange.fetchOpenPositions | fetch_open_positions |
| BitgetExchange#fetchUtaOpenPositions | fetch_uta_open_positions |
| BitgetInverseExchange.fetchBalance | fetch_inverse_balance |

These are executable method bodies over supplied receiver/HTTP/native-value
operations. They retain client/routing selection, account fallback, numeric
precedence, error/catch boundaries, serial metadata traversal, fake-position
construction/spread/deduplication, and inverse multi-coin USD aggregation.
Opaque native values retain reference identity, UTF-16 strings and nonfinite
numbers. Ordinary coin assignment and object-literal own-property definition
are separate operations. The 43 exemptions identify real binding/helper items;
no source row or Rust target was fabricated.

Concrete Exchange/HTTP assembly remains pending. The host binds JS array,
property, object, numeric/callee, iterator and promise operations; the fixture
is not an implementation or proof of all those bindings. `fetch_balance` takes
the selected Boolean preferFast argument; the source options/default/destructuring
prologue is a caller-binding obligation. Separately mapped numeric/position
callees retain their own manifest contracts. Arbitrary proxy/getter/prototype
or overridden-method behavior, promise scheduling, live network behavior and
full Exchange assembly are not established here.

## Native expectations and actual-source floors

`tests/account_positions.rs` checks decoded native variants/fields directly,
including number bits/NaN, UTF-16 units, enum strings, IDs, leverage, zero-position
fields, coin totals, independent field/row/request/emission counts and operation
order. Alias assertions cover cached balance, response info, original open
positions and the unchanged no-symbol array. Error markers/messages check tested
disposition; generic original-error forwarding is code inspected, not a native
pointer-identity gate. Native assertions precede each source comparison; no
roundtrip comparison is used as native evidence.

`account-positions-capture.mjs` extracts all eight actual field initializers.
It uses source ENDPOINTS/enums/numeric helpers/createPositionId, real p-iteration
mapSeries and lodash flatten/uniqBy. Receiver/HTTP/mapping collaborators are
fixture operations. Endpoint aliases with identical paths are validated against
the actual expected source table path, not the first matching table name.

The main floor is **64** independently prescribed cases (8 methods x 8 scenarios),
stopping at index **63**, `inverse-balance/fast`. The diagnostic floor is **7**,
stopping at index **6**, `uta-open/uta-list-failure`. Those cover two testnet reads,
changing/throwing second Classic data reads, and changing/throwing guarded UTA
data/list reads. Six additional native-only executions cover live serial-array
append, intermediate/trailing holes, unpaired UTF-16 symbols, NaN inverse total,
the production-mode mismatch message and unequal one-way isolated leverages.
The environment-mode enumeration stopped at its sole fixed false case; changing
false-to-true is covered by the separate diagnostic witness. None is claimed
exhaustive. Five test functions passed at the code SHA above.

## Gates measured at 874b05a537e58f183181410f1dcc0dc099314218

All Cargo commands use `/opt/homebrew/opt/rustup/bin` on PATH and `--jobs 2`,
with one Cargo process at a time. Source tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Measured exit / output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0, Finished dev |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` | 0, all crate unit/integration/doc tests passed |
| Scoped `--test account_positions` after byte restoration | 0, five passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, would update 0 manifests; output inspected |
| Rustfmt check of the two new Rust files; `git diff --check` | 0 |
| Python3.11 tomllib identity/status census | 0, eight changes, all pending -> ported |

The initial manifest writer ran after the edits before the initial commit. Its
pre-commit result is not claimed as a gate measured at 874b. The post-merge writer
is measured and bound to its own SHA below.

Initial fixture runs failed while the source script was incomplete, while an
endpoint alias was labelled by the first matching path, and while the diagnostic
expected floor still said 2 after expansion to 7. Those were corrected; they are
not mutation gates or final positive exits.

## Compiling controls at the same code SHA

Each control edited only the production body, ran the indicated named test,
required Finished test (successful compilation), observed exit **101** at a
native assertion, then restored the original bytes. No measured survivor.
The first five run `account_positions_native_then_source -- --exact`; the last
runs `native_diagnostic_getter_order_then_source -- --exact`, using scoped Cargo
`--test account_positions`. After all restorations, all five file tests exited 0.

| Family / mutation | Named native failure (exit 101) |
| --- | --- |
| Balance total: subtract available instead of upnl | `ACCOUNT native numeric balance/normal.total` |
| Classic metadata: swap long isolated leverage to sell | `ACCOUNT native numeric metadata/normal.0.leverage` |
| Synthesized position: set zero fields to 1 | `ACCOUNT native numeric positions/normal.1.entryPrice` |
| UTA position mapping: choose Classic callback | `ACCOUNT native scalar uta-open/normal.0.uta` |
| Inverse aggregation: subtract free USD accumulation | `ACCOUNT native numeric inverse-balance/normal.free` |
| Guarded Classic data: reuse first read instead of reading again | `ACCOUNT native numeric second classic data getter value.used` |

Earlier five-family controls also caught the initial `8de0bbce160a38ca69fff4e27fc09e5747638764`
code with compiling 101s and restored tests 0. The table above was rerun after
all review fixes and belongs to the final reviewed code SHA, not that ancestor.

## Sub-agent review

`/root/account_read_review` reviewed exact committed blobs at
`874b05a537e58f183181410f1dcc0dc099314218`. Final citation: all three repeated-getter
P2 findings resolved; no additional blocker in the eight inspected bodies within
the declared host contracts. The fixes preserve two ordered testnet reads,
Classic Array.isArray's guarded second response.data read, and UTA's guarded
second response.data/list reads.

Reviewer independently ran actual-source seven-input diagnostic capture and a
Python3.11 decoded-output checker: both exit 0. Reviewer checked eight mappings
and 43 unique helper exemptions. Reviewer did **not** run Cargo, the main 64-case
test, extractor, compiling mutations, clippy/wasm, CI or full suites. Its bounded
review does not prove concrete native JS/HTTP bindings or scheduling.

## Merge-down and post-merge gates

Integration `845cd310a963e53fc50dd9180a152e034262b6b4` was merged down as
`98455af15d35aed99540c9529f6a30c9e817b73b`. The exemption append conflict was
resolved with the exact integration text plus our 43 records; Python3.11 verified
all integration records and all our records remain, TOML parses, rs keys unique.

`git diff --exit-code 874b05a537e58f183181410f1dcc0dc099314218 HEAD --
sdk-rs/safe-cex-venue-bitget/src sdk-rs/safe-cex-venue-bitget/tests
sdk-rs/safe-cex-venue-bitget/scripts` exited **0** at the merge SHA. Reviewed
source, tests and scripts are byte-unchanged. Shared exchange/Cargo.lock inputs
changed. Scoped all-targets Cargo check at this merge SHA exited **0** (Finished
dev). The full Bitget crate suite with `-- --include-ignored` at this merge SHA
also completed with exit **0**, including the five new file tests and doc tests. Both extractor writer and extractor check at this merge
SHA exited **0**, 7341 symbols, zero manifest changes; output inspected. A second
Python3.11 tomllib census against integration 845cd also exited **0**, the same
eight changes and 71/80/76 triple. Controls were not rerun after this
merge and remain explicitly attached to 874b above.

Final fetch/merge: integration `b5ac6229e2502e48106b0a549da0524583b04627`
merged as `1274b3d425ca31eee29fd82d411542c84d88da74`. No conflict. Both scoped
preservation commands exited **0**: the original reviewed-code source/tests/scripts
comparison against 874b, and the post-merge gate comparison against 98455 for
crate source/tests/scripts, Cargo.lock, direct SDK dependency crates and the
Bitget/types/math/position-ID TypeScript source inputs. Only other venue work,
metadata and package scripts changed in this merge. Cargo/controls were not rerun
for this final merge; their actual measured SHAs remain those stated above.

Extractor check at **1274b3d425ca31eee29fd82d411542c84d88da74** exited **0**;
7341 symbols, would update zero manifests, zero additions/orphans, output read.
Final Python3.11 tomllib census at that SHA against b5ac integration exited **0**:
eleven manifests, 227 identical identities, **8 pending -> ported, 0 -> divergent**;
**71 ported / 80 divergent / 76 pending**. The delivery commit after this merge
changes this evidence file only; it does not claim additional gate executions.

Not run: workspace/monorepo builds or suites, host clippy, wasm clippy/check,
CI reruns, live/network tests, full native JS/HTTP/Exchange binding verification,
exhaustive getter/proxy/prototype/scheduling enumeration. No PR was created.
