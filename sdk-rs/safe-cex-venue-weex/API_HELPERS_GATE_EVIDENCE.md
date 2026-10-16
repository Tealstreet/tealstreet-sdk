# WEEX signing and configured retry callbacks

Authoring parent: `f436fc816390a8b4556ab7a2fe0ae85072337036`.
Production/test gate revision: `8dd7b1bf74951650b4987a559ef93eacff18adbb`.
Post-merge gate revision: `290e4e10c1f5ceb61ea742ed73c17953e667aa15`, merging
integration `4e0ca1accd6fc8a81fb6d163581861c9860f8da9` after metadata correction
`a6b34214ca97b51b76cf1f171a666d64ef9cc59f`.
No unrelated unpublished lane work was used as an ancestor.

## Measured movement

Four pending to ported, zero to divergent. Independent fixed expected symbols:

| TypeScript row | Native target |
| --- | --- |
| signWeexMessage | signing.rs::sign_weex_message |
| WEEX_RETRY_CONFIG | retry.rs::WEEX_RETRY_CONFIG and retry.rs::WeexRetryConfig |
| WEEX_RETRY_CONFIG.retryCondition | retry.rs::weex_retry_condition |
| WEEX_RETRY_CONFIG.retryDelay | retry.rs::weex_retry_delay |

Python tomllib compared `git show` rows against that fixed list, not the
artifact's own length: exit 0, both before merge and after merge. The authoring
parent and merged integration each have 31 ported / 26 divergent / 99 pending
WEEX rows; the post-merge revision has 35 / 26 / 95, across 156 rows.

The signing port represents the full source string domain as UTF16, retaining
paired and unpaired surrogates before Node-compatible UTF8 conversion. It uses
workspace HMAC-SHA256 and padded standard base64. Native returned signatures
are compared directly with the actual TypeScript declaration and installed
create-hmac; this is not a wire encode/decode comparison.

The config retains retries=3, shouldResetTimeout=true and both callable fields.
Condition delegates the already-mapped typed policy and imported classifier
capability. Delay preserves strict numeric status429 selection and result
forwarding. Its captured getWeexRateLimitRetryDelayMs capability is an explicit
dependency: that calculator remains pending. No Axios plugin installation,
arbitrary getter runtime or mutable JavaScript object runtime is claimed ported.

## Observed gates

All cargo commands ran serially. Oracle runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured SHA | Exit/result |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 8dd7b1bf74951650b4987a559ef93eacff18adbb | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/api-helper-mutations.py | 8dd7b1bf74951650b4987a559ef93eacff18adbb | 0; four compiled failures and individual restored passes |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 8dd7b1bf74951650b4987a559ef93eacff18adbb | 0; zero updates/added/orphaned, 7341 symbols |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 290e4e10c1f5ceb61ea742ed73c17953e667aa15 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 290e4e10c1f5ceb61ea742ed73c17953e667aa15 | 0; invention/status gates passed, 156 rows, 68 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 290e4e10c1f5ceb61ea742ed73c17953e667aa15 | 0; zero updates/added/orphaned, 7341 symbols |

Initial parity-check at 8dd7b1bf74951650b4987a559ef93eacff18adbb exited 1:
two callback rename notes lacked the exact Rust target. Corrected only those
notes in a6b34214ca97b51b76cf1f171a666d64ef9cc59f, then reran the gate after
merge. No gate was weakened and the initial failed result is not a pass.
Extractor writer ran after each manifest edit; the last write reported zero
updates/added/orphaned. Draft-tree test/write runs are not final-SHA proof.

`git diff --exit-code 8dd7b1bf74951650b4987a559ef93eacff18adbb
290e4e10c1f5ceb61ea742ed73c17953e667aa15 --` the WEEX src/tests/Cargo.toml,
safe-cex-types, safe-cex-num, safe-cex-store and original WEEX TypeScript paths
exited 0. Production/test inputs are byte-unchanged, so the measured tests and
controls retain their original SHA binding. Post-merge metadata gates and
compile floor were measured separately above.

## Native assertions and controls

The first corpus iteration stops at 13 signing cases, 8 configured condition
cases and 9 configured delay cases: 30 results, plus two config fields. Fixed
ordered identities and counts are asserted independently. Native strings,
booleans, f64 bits, six classifier calls and five delay calls are asserted;
there is no wire roundtrip. This finite enumeration is a coverage floor.

Each selected control compiled and exited 101 at the named native assertion,
then restored the original bytes and passed the affected target with exit 0:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test api_helpers`, test `api_helpers_match_actual_typescript`.

| Production mutation | Failed native assertion | Restored exit |
| --- | --- | --- |
| Append key bytes to the signed message | api_helpers::signature::ascii | 0 |
| Config retries 3 to 4 | api_helpers::config::retries | 0 |
| Invert configured condition result | api_helpers::condition::network_get | 0 |
| Add one to delegated delay result | api_helpers::delay::rate | 0 |

All four selected controls caught; zero survivors in this iteration. No other
mutations were enumerated. The script refuses compiler failures as controls.

Sub-agent weex_review found a compile-only hazard in the initially proposed
signing mutation before execution. Corrected it to retain both variables; the
four compiled failures above measure the corrected script. Review found no
production parity issues and ran no cargo, tests, mutations, edits or commits.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live exchange calls or CI reruns. No income pager was added.
Ignored local logs: sdk-rs/target/weex-api-helper-gates/.

## Delivery merge comparison

Immediately before delivery, fetched and merged integration `61c78429c39fc37b14d3fb5d479572ec74ab9c29`
into merge revision `4330e29c45af6ec2c57290a026bde14135d5cf20`. The first fetch encountered a
shared remote-ref update race; the same fetch succeeded on retry. No force update
was used.

Python required git diff --exit-code from post-merge gate revision
`290e4e10c1f5ceb61ea742ed73c17953e667aa15` to this delivery merge to exit 0 for
WEEX src/tests/Cargo.toml/manifests, original venue TS, shared Rust dependencies,
workspace Cargo.toml/Cargo.lock, parity exemptions, extractor code and checker
code. It did: those gate inputs are byte-unchanged. The earlier measured gate
exits retain their named SHA bindings; no tests or controls were rerun at the
delivery merge. Other venues' manifests changed with integration.

Independent TOML comparison against this fetched integration again matched
exactly the four expected pending-to-ported transitions, exit 0; counts remain
35 ported / 26 divergent / 95 pending. This narrative-only descendant adds no
production/test change.
