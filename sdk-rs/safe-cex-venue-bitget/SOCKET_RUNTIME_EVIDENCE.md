# Bitget socket dispatch, authentication and reconciliation

Fresh branch `feat/rust-bitget-socket-runtime`; actual direct integration parent
`95b56a79bc646156e1812b17428e0fb6295413a5`. No separate unreviewed lane branch
was used as an ancestor. Body SHA: `445b5799b9613889d323566bf410036916638ee0`.
Final test/gate/review SHA: `6ca232f1466ef4dcf27ad374ffbb7f9e46f9a207`;
the follow-up changes only native crypto argument assertions in the test.

## Census and actual targets

Python3.11 **tomllib** compared `(manifest, kind, ts)` identities against that
parent: **8 pending -> ported, 0 -> divergent**. Eleven manifests, 227 identical
identities; **95 ported / 80 divergent / 52 pending** before, **103 / 80 / 44**
after. Enumeration stopped at sorted file eleven, `bitgetusdc.exchange.ts.toml`.
Census is separate from behavioral proof.

Eight targets in `src/socket_runtime.rs`:

| TypeScript field | Rust body |
| --- | --- |
| BitgetPrivateWebsocket.connectAndSubscribe | connect_and_subscribe |
| BitgetPrivateWebsocket.onMessage | on_private_message |
| BitgetPublicWebsocket.onMessage | on_public_message |
| BitgetPrivateWebsocket#executePositionFetch | execute_position_fetch |
| BitgetPrivateWebsocket#executeBalanceFetch | execute_balance_fetch |
| BitgetPrivateWebsocket#executeOrdersFetch | execute_orders_fetch |
| BitgetPrivateWebsocket#auth | auth |
| BitgetPrivateWebsocket#subscribe | subscribe |

Bodies retain counter/fallback/UTA URL behavior and listener capture order;
private track/diagnostic/pong order and heartbeat exclusion versus public
heartbeat increment; private parsing and dispatch even when disposed; login
formatting, UTA topic precedence, Classic normalization, ack suppression; public
substring/parsed guards, ticker early return, candle/books independent parses,
computed topic keys, repeated handler reads and live bare callbacks. Fetch
executors retain timestamps/reset before requests, virtual request clock versus
wall clock, post-await disposed/stale guards, silent catch, outer store callable
capture before merge, loaded copy, tombstone/current-terminal suppression and
WS algo/recent-open preservation. Auth retains repeated signer/error getters,
request/login schema, Remote WS signing unavailable wording, local HMAC chain
and branch-specific getter/capture ordering. Subscription keeps UTA topics
versus four Classic channels with independent uppercase product reads.

The 30 exemptions are actual native receiver/callable-owner/value/iterator/
array/map/set/clock/crypto/imported-operation and async-work binding helpers.
Opaque clones retain references and native numeric/UTF-16 values. Captured
member owners, original imported/native operations and independently owned
started work are supplied contracts; message argument prologues supply data,
and async bodies execute at the original execution/poll boundary. Concrete
socket/Exchange/HTTP assembly, eager scheduling/ownership/cancellation, arbitrary
native overrides/proxies/prototypes/coercion and broad crypto/input domains
remain separate binding obligations. Existing callees keep their own mappings;
no class or constructor target was fabricated. No shared crate type/trait changed.
The only dependency addition is dev-only safe-cex-crypto for the native HMAC witness.

## Native assertions and actual-source floors

Six tests in `tests/socket_runtime.rs` assert decoded native variants, independent
counts, numeric bits, UTF-16, state/reset values, request clocks/sequence,
reconciled row fields/order, auth/remote/subscription objects before stringify,
crypto invocation arguments and original error Rc identity before source/wire
comparisons. Position/balance/reconciled-order/parsed-JSON/bare-callback aliases
and loaded-copy behavior have direct native reference assertions. No expected
string serialization or decode/re-encode comparison supplies the payload gate.

`socket-runtime-capture.mjs` runs the eight actual field initializers and actual
normalizeChannel/jsonParse helpers, enums and private URL constants. Date,
virtual clock, routing, nonce, receiver/store/signing/reconciliation collaborators
are fixtures. Source local HMAC uses actual Node create-hmac; native fixture uses
actual Rust safe-cex-crypto. Independent Python HMAC-SHA256/base64 expectation for
key `secret`, text `1234GET/user/verify` is
`ntCOgO3bBOD6SxGsLfbQ98zohwl0Xh+f/HLJyVTbGwQ=`. Native checks separately assert
one create/update/digest and complete arguments [sha256,secret],
[1234GET/user/verify], [base64], so fixture output policy cannot mask changed
algorithm/encoding arguments. This is one ASCII signing vector, not the full
crypto/coercion/byte-input domain. Nonce is supplied, not RNG proof.

The **52** actual-source cases are bounded floors:

| Group | Independent floor | Stopped at |
| --- | --- | --- |
| Private connection | 6 | index 5, swallowed fallback telemetry error |
| Private subscription | 2 | index 1, UTA topics |
| Authentication | 6 | index 5, second error getter changes text |
| Three fetch executors x five scenarios | 15 | index 14, stale order reply |
| Private dispatcher | 13 | index 12, subscription ack |
| Public dispatcher | 10 | index 9, cb1 appends cb3 to live handler array |

Fetch cases cover normal/rejection/post-await disposal/prefix setter error/stale
sequence, seven independently expected reconciled rows and original aliases,
TTL pruning and timestamp differences, fast balance options and loaded immutability.
Fetched orders include a hole that filter skips. Dispatch cases include pong,
invalid/login/UTA/Classic/ack/disposed frames, ticker guard, whitespace that fails
the literal substring guard, candle/books double parse and appended callback.
These are not all error sites, array shapes, status/input domains or timings.

## Nonblocking connection-listener coverage finding

The connection fixture allocates a fresh handler `{id}` value for each getter.
Native assertions prove listener event/handler selection and source comparisons
prove observed order; they do **not** retain the exact getter result for an Rc
identity assertion. Call site: BitgetPrivateWebsocket.connectAndSubscribe,
`this.ws.addEventListener(event, this.onOpen/onMessage/onClose)`, Rust
`connect_and_subscribe` receiver/call sequence. Review found that production
passes the returned references correctly. This is a test coverage gap, not a
demonstrated production divergence or a measured mutation survivor. Other alias
claims above have separate direct native assertions. Callback callability and
native owner binding remain unproved runtime obligations.

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed
**445b5799b9613889d323566bf410036916638ee0**, verified parent
**95b56a79bc646156e1812b17428e0fb6295413a5**, and found no production-body
mismatch/blocking mapping issue in eight rows. It compared source guards/read/
capture order, dispatch precedence/pong/disposal, live bare callbacks, fetch
reset/clocks/stale/preservation/catches, signer/error rereads and schemas/HMAC.
It inspected fixed floor 52 and identified the listener-identity gap above.

Reviewer measured committed git reads **0**, TOML eight transitions/30 unique
helpers/dev-only crypto placement **0**, independent Python HMAC vector **0**,
and committed-source auth floor six (original rejection/request/login fields,
rotating signer, second error read and actual HMAC) node/Python **0**. The script
loader root was rebased for node -e; five TS dependency blobs were verified
byte-identical. No edits. Reviewer did not run Cargo/native targets, extractor,
mutation controls, host/wasm clippy, CI or workspace checks.

The same sub-agent reviewed exact follow-up
**6ca232f1466ef4dcf27ad374ffbb7f9e46f9a207** and confirmed independent native
crypto counts/argument arrays precede wire/source comparisons and close the
algorithm/encoding assertion gap. Production/script/mappings/exemptions/Cargo
files are byte-unchanged from 445 (reviewer diff/read exits **0**). No runtime
measurements at 6ca by reviewer; prior vector/source executions remain tied to
445. The listener limitation remains. Review does not cover intervening
integration commits.

## Claims not run or not established

No workspace compilation, repo-wide suites/builds, host clippy, wasm clippy,
CI rerun, live credential/network tests or end-to-end venue execution. Concrete
runtime assembly, temporal/eager ownership/cancellation, connection getter-return
identity, arbitrary native operation/proxy/prototype/coercion behavior, full JSON/
calendar/crypto byte/input domains and RNG behavior are not established by these
fixtures. Pending class/assembly/callee rows remain pending; these statuses cover
actual executable bodies over their declared supplied-operation contracts.

## Measured positive gates at the test SHA

These exits were measured at exact committed
`6ca232f1466ef4dcf27ad374ffbb7f9e46f9a207`, before integration merge:

| Gate | Actual exit and output |
| --- | --- |
| Scoped Cargo check | **0**, `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2`, Finished dev |
| New native/source tests | **0**, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test socket_runtime --jobs 2`, six passed, 52 bounded source cases |
| Full Bitget crate tests including ignored | **0**, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored`; all emitted test-result summaries passed; doc-test floor zero |
| Manifest writer | **0**, `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`; output read: 7341 symbols, zero updated/added/orphaned |
| Extractor checker | **0**, same node/yarn entry with `parity:extract:check`; output read: zero updated/added/orphaned |
| Independent tomllib census | **0**, 11 manifests / 227 same identities / eight pending-to-ported transitions against actual parent; last file stated above |
| Source/test/script byte restoration check | **0**, scoped `git diff --exit-code 6ca232f1466ef4dcf27ad374ffbb7f9e46f9a207 -- <crate>/src <crate>/tests <crate>/scripts` |
| Formatting and patch whitespace | **0**, `rustfmt --check --edition 2024` for the two new Rust files and `git diff --check` |

Cargo commands used PATH `/opt/homebrew/opt/rustup/bin:$PATH`, at most one
Cargo at a time and `--jobs 2`. Tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Logs were captured without an exit-masking pipe; each process completion was
observed. The full crate run was not a workspace run.

An initial precommit positive run exited **101** because source fixture tracing
omitted actual jsonParse and accountName getter events. Those fixture mismatches
were corrected and the committed tests above passed; this is not a mutation
control or production discrepancy. A later rustfmt check mistakenly specified
edition 2021 and exited **1**; the workspace declares 2024, whose check passed
**0** without modifying files. Earlier body-commit check/writer/checker exits at
445 were **0**; the table uses the final test SHA measurements.

## Discriminating controls at 6ca232f146

Each control separately changed only the named production expression from the
exact committed test SHA. Each compiled (`Finished test`), exited **101**, and
failed the following native assertion before source/wire comparison. The original
bytes were restored after every control. No survivor or compilation-only control
was omitted.

| Control | Test selector | Named native failure |
| --- | --- | --- |
| connect-uta | `native_connect_then_source` | assertion `left == right` failed: SOCKET native UTF16 connection URL |
| private-handler | `native_private_dispatch_then_source` | assertion `left == right` failed: SOCKET independent handler count |
| public-handler | `native_public_dispatch_then_source` | assertion `left == right` failed: SOCKET independent handler count |
| position-clock | `native_fetches_then_source` | SOCKET native numeric position request clock: 3000 != 2000 |
| balance-loaded | `native_fetches_then_source` | SOCKET native scalar balance patch.loaded.balance: Bool(false) != Bool(true) |
| order-tombstone | `native_fetches_then_source` | assertion `left == right` failed: SOCKET native independent array count reconciled orders |
| auth-algorithm | `native_auth_then_source` | assertion `left == right` failed: SOCKET native UTF16 local crypto native arguments crypto.create.0 |
| subscribe-channel | `native_subscribe_then_source` | assertion `left == right` failed: SOCKET native UTF16 subscription payload.args.3.channel |

After all eight restorations, the complete `--test socket_runtime` run exited
**0**, six passed. A subsequent scoped committed-source/test/script diff and
edition-2024 rustfmt check both exited **0**. All eight mutation selectors used
`--exact`; these eight controls are a bounded floor, stopping at subscription
channel (control index 7), not an exhaustive mutation enumeration.

## Integration merge and gate preservation

Fetched and merged integration **7078883cf0aa0de5f11f253c7cee83d98be36265**
into this branch, merge SHA **5c441dc14b5af69b949aac6c369459d73d4a02f9**.
The only conflict was `sdk-rs/parity/exempt.toml`: resolution used the complete
stage-3 integration file plus the 30 generated actual Bitget records. Python
tomllib asserted all integration rows retained in order, all own rows retained,
and unique Rust symbols; exit **0**. No CLAUDE conflict or lane ancestor was added.
Integration commits outside this slice are not covered by the cited review.

At exact merge SHA **5c441dc14b5af69b949aac6c369459d73d4a02f9**:

- `git diff --exit-code 6ca232f1466ef4dcf27ad374ffbb7f9e46f9a207 HEAD --`
  crate `src`, `tests`, `scripts`: **0**, byte-identical.
- Recursive local dependency source/build inputs (candle-cache, crypto, exchange,
  num, store, transport, types), workspace Cargo.toml/.cargo, venue Cargo.toml,
  Cargo.lock, all safe-cex TS and json-parse TS, yarn.lock: **0**, byte-identical.
- The wider input diff including package.json exited **1**. The complete diff
  was solely the unrelated `venue-oracle:kucoin` script adding public_streams.
  Python JSON comparison after removing that single script key exited **0**;
  Cargo/oracle/extractor commands used here are unchanged.
- Manifest writer **0**, output read: zero updates, additions or orphans.
- Extractor checker **0**, output read: zero would-update, additions or orphans.
- Independent tomllib census against merged integration **0**, exactly eight
  pending -> ported and zero divergent, 11 files / 227 same identities,
  95/80/52 -> 103/80/44; stopped at sorted file eleven named above.
- `git diff --check`: **0**.

Cargo, restored native tests and mutation controls were **not rerun after this
merge**: their exact source, Rust dependency/build/lock inputs and TypeScript
oracle inputs are byte-identical. Their exits remain attached to **6ca232f146**,
not claimed as fresh measurements at the merge or the following evidence-only
commit. Extractor exits above are fresh measurements at **5c441dc14b**.
A final pre-push fetch still observed integration **7078883cf0**.
