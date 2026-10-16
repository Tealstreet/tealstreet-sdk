# WEEX websocket control evidence

Authoring parent: `d8ab034625643795919e84c63ee6e5d764cb11df`.
Measured implementation SHA: `a0a8d035f3e66c35ae93d295d4dd639363571ab3`.
Fresh branch: feat/rust-venue-weex-ws-control.
No unrelated unpublished lane work was used as an ancestor.

## Row movement

Seven pending to ported; zero to divergent:

| Source row | ws_control.rs target |
| --- | --- |
| WeexPrivateWebsocket#createAuthHeaders | create_auth_headers and WeexAuthHeaderRuntime |
| WeexPrivateWebsocket#subscribe | private_subscribe, WeexWebsocketControlState and WeexWebsocketControlRuntime |
| WeexPrivateWebsocket#logUnsupportedTransport | log_unsupported_transport |
| WeexPrivateWebsocket.onOpen | private_on_open |
| WeexPrivateWebsocket.ping | private_ping |
| WeexPublicWebsocket.onOpen | public_on_open |
| WeexPublicWebsocket.ping | public_ping |

Python tomllib read committed manifests through git show and required this
independent fixed set of seven pending-to-ported transitions, with no other
status changes: exit 0. It checked all ten targets against actual committed
public production definitions. Both full websocket class rows remain pending.
Counts across 156 rows changed from 55 ported / 26 divergent / 75 pending to
62 / 26 / 68. Extractor writer normalized two manifests, zero added/orphaned
rows. Mapping arrays were serialized programmatically then writer-normalized.

## Scope and behavior

Control functions project declared numeric/bool state and ordinary callback
bindings. Clocks, JSON.stringify/sendWithLog, warning logging, snapshot startup
and reconnection notification are explicit fallible host capabilities. Native
public reopen handling calls the already-ported subscription functions directly.
Both ping wrappers share the internal source-identical algorithm.

Auth assembly returns the five native own header entries in source order,
formats the Number timestamp with the shared JS number formatter, reads key,
passphrase and secret in source order, and signs timestamp+PRIVATE_WS_REQUEST_PATH.
Generic values preserve undefined key/passphrase values. The fixture signer
calls the already-ported real Rust signing function for strings and models the
actual TypeError for undefined secret; it is not fed oracle signatures.
UTF16 credentials and reasons retain lone surrogates.

Subscription increments the ID before serializing/sending, without a disposal
guard. Unsupported logging sets its flag before the warning call and preserves
that flag on failure. Private open guards once, subscribes, starts a snapshot
with limit 10 without awaiting it, then emits. Ping guards once, sets pingAt
from performance.now, increments ID, then sends. Public open resets the hourly
budget/log flag before ticker and tracked-topic resubscription, then emits.
State changes already performed survive later callback errors.

Full websocket classes/constructors, arbitrary class field/method accessors,
transport/header-object integration, host engine primitives and asynchronous
snapshot completion/rejection remain pending. The snapshot capability models
synchronous invocation/startup, not its eventual Promise result. No network
authentication, complete JavaScript runtime or actual Store audit is claimed.

## Gates at a0a8d035f3e66c35ae93d295d4dd639363571ab3

All cargo commands ran serially. Oracle environment:
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Command | Measured exit / result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/ws-control-mutations.py | 0; seven compiled assertion failures, each individually restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 106 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

Mutation runs cover only the affected target:
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test ws_control; named test ws_control_matches_actual_typescript.
The SHA-bound passes are the seven restored target runs. A draft target also
passed before the implementation commit and is not counted as SHA-bound proof.

## Native proof and discrimination

ws-control-oracle.cjs extracts the actual five private fields, two new public
fields, five already-ported public dependencies, four constants and actual
signing declaration from TypeScript ASTs. It compiles the actual bodies with
actual shared venue constants/LogSeverity and the source's create-hmac dependency.
Function bodies and constant values are not copied into the oracle.

Independent pre-call descriptors drive native hosts. Native header values and
ordered names, sign arguments, state, method/params/ID packet fields, log text
and snapshot limits are asserted before all traces. Source packet fields are
captured before actual JSON.stringify. There is no native encode/decode
roundtrip or wire comparison. Literal expectations require all five header
names, account/positions/orders/fill channel order, one warning across two log
calls, and two new operations/sends after a public budget reset.

The first corpus iteration stops at 16 auth cases, 30 control cases and eight
public-open cases: 54. Fixed ordered IDs and literal group/total counts are
independent of the artifact. Auth cases cover zero/signed-zero/nonfinite and
exponent timestamps, Unicode/NUL/lone-surrogate values, absent credentials,
clock/getter errors and credentials changed during the clock callback. Control
cases cover default/empty/UTF16 reasons, logged-once behavior, disposed guards,
nonfinite/signed-zero IDs, serializer/sender/clock/snapshot/emitter errors and
synchronous disposal during sending/performance callbacks. Public cases cover
resetting a full budget, duplicates and partial state on dependency errors.
This is a coverage floor. Other callback mutations, arbitrary accessors,
Promise settlement and complete host primitive behavior were not enumerated.

Each selected production mutation compiled and failed its named native
assertion with exit 101, restored original bytes, then passed with exit 0:

| Mutation | Named assertion | Restored target exit |
| --- | --- | --- |
| Change Tealstreet user agent case | auth/basic::header_value | 0 |
| Change fill channel to fills | subscribe/basic::params | 0 |
| Leave unsupported warning flag false | log/default::logged | 0 |
| Start snapshot with limit 11 | private_open/basic::snapshot_limit | 0 |
| Skip non-disposed private ping wrapper | private_ping/basic::id | 0 |
| Skip non-disposed public ping wrapper | public_ping/basic::id | 0 |
| Leave public subscription-limit flag true | public_open/basic::logged | 0 |

The script requires exit 101, the named assertion and assertion left==right
failure text. It restores in a finally block and passes after each restoration.
Seven controls caught; zero survivors among these controls. Other mutations
were not enumerated; compiler failures are not accepted as discrimination.

Sub-agent weex_review inspected actual source, implementation, native fixtures,
all seven mappings and controls; no concrete findings. It ran no cargo, tests
or mutations and made no edits or commits. Its review does not prove full host
async behavior or arbitrary getter/Promise rejection handling.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. No income pager was added.
Ignored local logs: sdk-rs/target/weex-ws-control-gates/.

## Delivery merge

Fetched this worktree's FETCH_HEAD as integration
`c4c1ec196d794b431bed97b2ede5eff17a41c6f5`, then merged that exact SHA into
`c6974511c19fa28ab0c60063cdc7b175816cdcd0`, with no conflicts.
A committed-TOML audit against the actual second merge parent exited 0 and
required the same seven transitions and counts above.

Git diff --exit-code from the measured implementation SHA to this merge exited
0 for WEEX src/tests/Cargo.toml/manifests, workspace Cargo manifest/lock, shared
types/num/store, actual WEEX TypeScript/shared enums/logging enum and Cargo
configuration paths. Original cargo and mutation measurements remain bound to
the original SHA; they were not rerun at the merge because these inputs are
byte-unchanged. Package scripts and global exemptions changed, so affected
gates were remeasured at the merge SHA:

| Command | Exit at c6974511c19fa28ab0c60063cdc7b175816cdcd0 |
| --- | --- |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 106 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, zero updates/added/orphaned |

This evidence commit changes only this narrative.
