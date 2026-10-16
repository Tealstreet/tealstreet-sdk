# OKX policy and endpoint rows

PR 7442 moved **6 pending to ported, 0 to divergent** at code SHA
`322055a329bdc6253db74b6218e5bbee05dac00b`, based on actual integration
`76fd7193d444bc6bb760c03050db4a9b217f73cb`. Python 3.11 tomllib parsed all
twelve OKX manifests: 36 ported / 16 divergent / 111 pending became 42 / 16 / 105.
The methods cover standard/EU market endpoint selection, EU instrument types,
position side and business resync/statistics policy. The PR has since merged.

This continuation moves **2 additional pending to ported, 0 to divergent**:
private and business websocket endpoint getters. Tested merge SHA:
`1fcb23eef3d13a0d2375c51edf8eee3fe4efd1f6`, whose parents are getter commit
`0805c1e9df08860aa4cab854006fd832a7cb29ff` and integration
`54851c81b7c780ebfea3682883bd3a49e12b9d8f`. Merge-down completed without conflicts
before delivery. No unreviewed feature ancestor was adopted. Against that
integration, tomllib gives **42 / 16 / 105 -> 44 / 16 / 103**.

Gates completed separately at the named SHAs, with --jobs 2 and only one cargo
process at a time:

| Gate | Initial six, 322055a329 | New two, 1fcb23eef3 |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0, zero errors | 0, zero errors |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test family_policy` | 0, one test passed | 0, one expanded test passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 0 |

Both extractor outputs were inspected: 7341 symbols / 512 source files;
zero changed manifests, added or orphaned symbols. Mapping/provenance updates
used extractor parseManifest/serializeManifest, followed by parity:extract:
exit 0 with zero updates. No arrays were hand-spelled. The existing OKX oracle
discovers the new test automatically; package.json was not edited.
Rustfmt --check on the changed Rust files exited 0 at the continuation SHA.

One compiling production control per initial row ran the exact
six_live_policy_rows_match_native_values_before_wire test at 322055a329:

- Standard endpoint switched to ACCOUNT_INSTRUMENTS: 101 at `OKX-POLICY native standard endpoint`.
- EU FUTURES value replaced by SWAP: 101 at `OKX-POLICY native EU instrument identity`.
- EU authenticated endpoint switched to MARKETS: 101 at `OKX-POLICY native EU account endpoint`.
- Conditional reduce-only exclusion inverted: 101 at `OKX-POLICY native position side Some(Hedged)/Market/Buy/Some(true)`.
- Statistics topic changed from orders to positions: 101 at `OKX-POLICY native order stats topic`.
- Business resync true changed to false: 101 at `OKX-POLICY native business resync`.

At 1fcb23eef3, each new getter had its own compiling control, running the exact
eight_live_policy_rows_match_native_values_before_wire test. Private role index
1 changed to 2: **101** at `OKX-POLICY native private endpoint Okx/None`.
Business index 2 changed to 1: **101** at `OKX-POLICY native business endpoint Okx/None`.
After each of all eight controls, finally restored the source byte-for-byte
and the same exact test passed with **exit 0**. None survived. The initial six
controls are not relabeled as controls run at the later merge SHA.

All native values and complete fixed-error metadata are asserted before any
wire comparison. Independent literal expectations cover 108 position cases,
stopping at Hedged / TrailingStopLoss / Sell / true; two market endpoint modes;
and six websocket profiles / twelve URLs, stopping at Okxeu / true.
These are floors for the declared enum/optional-boolean domain, not a census
of arbitrary malformed JavaScript objects. The live AST oracle executes actual
methods, getters and the scalar constant with network constructors omitted.
No transport, task, Store write, class implementation or representation boundary
was introduced. Constructor, account loading and socket lifecycle rows stay pending.

Static sub-agent review confirmed the initial six and added two methods,
native assertion ordering, targets/provenance and oracle discovery, with no
findings; it ran no gates. Guidance and agent-memory were not re-read.
Not run for either slice: full venue suite, workspace build, host Clippy,
wasm build/Clippy, Rust parity tests, or CI. The following evidence-only commit
does not alter the tested code, tests, oracle script or manifests.

## Logged private and business pings

The next slice adds **2 pending to ported, 0 to divergent** for
OKXPrivateWebsocket.ping and OKXBusinessWebsocket.ping. Relative to integration
`d3e8141626c0ab4c235b3816a986144961349554`, the branch carries **4 pending to
ported** including the two endpoint getters above: **42 / 16 / 105 ->
46 / 16 / 101**. Python 3.11 tomllib parsed all twelve venue manifests.
The earlier eleven-file wording was a reporting error; the row totals stand.

Actual implementation parent was `a4ebc83f700fdd3560163d157c300a737b13d4ac`.
Merge `b6292d65cb9d66e96da59958d46f4e108781ea6c` has implementation parent
`107fde8ca3629a7d198a6ee0b146580a9671d303` and the integration parent above.
Scoped src/tests/manifests byte equality against 107fde8ca3 exited 0 after
that clean merge. The endpoint getter work is an unmerged ancestor carried
intentionally on the same delivery branch; no unrelated feature ancestor was
adopted. The final source/test gate SHA is
`5f49b83a234f79a51b855a7f4edf9563c747f539`, adding the reviewer-requested
socket readiness assertion to the merged code.

Both helpers preserve the disposal guard, monotonic timestamp assignment,
then fallible send_with_log("ping"). They use landed core interfaces only.
All native checks precede running the Node oracle: literal payload and effect
order, complete ExchangeError (including listener subscriber IDs), latency
observations of the retained timestamp, socket readiness and fixed dial count.
Sixty declared profiles stop at business / 2000.5 / disposed / closed-log-error:
two roles, five clock inputs (including signed zero), two disposal states,
and three outcomes. This is a floor, not a claim of arbitrary JS input coverage
or exact signed-zero identity inside inaccessible native timestamp state.
The AST oracle executes the live venue ping and live base sendWithLog members,
with injected send/diagnostic fixtures. Actual core send/error behavior is
asserted natively; full socket constructors and network lifecycle stay pending.

Static sub-agent review found that a dial count alone could not establish
absence of socket close. The test now pins readiness immediately after ping
and labels the dial assertion precisely. Reviewer confirmed the correction;
no remaining static finding. The reviewer ran no gates. The first uncommitted
test attempt exited 101 for an unused shared fixture method, corrected by
pumping and discarding scheduled tasks. The corrected pre-merge test exited 0;
that exit is not reported as a post-merge gate.

Measured gates at 5f49b83a23, sequential cargo with --jobs 2:

| Gate | Measured exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0, zero errors after all restores |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test logged_ping --test family_policy` | 0, two tests passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output inspected: 7341 symbols / 512 files, zero changes/additions/orphans |
| Rustfmt --edition 2024 --check for logged_socket.rs and logged_ping.rs | 0 |
| Scoped git diff --exit-code for src/tests/manifests after controls | 0 |

Compiling controls at 5f49b83a23 ran the exact
private_and_business_ping_native_before_wire test:

| Production change | Broken exit and named native assertion | Restored exit |
| --- | --- | --- |
| private_ping sends before marking the clock | 101, `OKX-LOGGED private native clock before send` | 0 |
| business_ping sends before marking the clock | 101, `OKX-LOGGED business native clock before send` | 0 |
| private_ping injects core reconnect before marking the clock | 101, `OKX-LOGGED private native socket remains open or absent` | 0 |

Finally restored the production file byte-for-byte after every control. None
survived. Earlier eight policy controls remain bound to their own measured
SHAs above. No full venue suite, workspace compilation, host/wasm Clippy,
wasm build, Rust parity suite, or CI was run. This evidence-only delivery
commit leaves tested src/tests/scripts/manifests byte-unchanged. KuCoin has
zero new rows in this delivery; its integrated work was not edited or re-gated.
