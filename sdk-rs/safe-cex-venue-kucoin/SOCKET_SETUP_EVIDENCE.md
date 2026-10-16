# KuCoin websocket setup evidence

Moved **9 pending -> ported, zero divergent** on `feat/kucoin-websocket-setup`.
Fresh actual parent: integration `3e4ab045924dd879fa2e3dfa0519d647808e99cd`.
No unreviewed feature branch was used as the parent. The prior message-routing slice
was already merged; none of its rows are counted again here.

## Mappings and implementation boundary

All nine targets are real public functions in `safe-cex-venue-kucoin/src/socket_setup.rs`:

| Source symbol | Rust function |
| --- | --- |
| `KucoinPublicWebsocket.connectAndSubscribe` | `connect_futures_public` |
| `KucoinPrivateWebsocket.connectAndSubscribe` | `connect_futures_private` |
| `KucoinSpotPublicWebsocket.connectAndSubscribe` | `connect_spot_public` |
| `KucoinSpotPrivateWebsocket.connectAndSubscribe` | `connect_spot_private` |
| `KucoinPublicWebsocket.reconnect` | `reconnect_futures_public` |
| `KucoinPrivateWebsocket.reconnect` | `reconnect_futures_private` |
| `KucoinPublicWebsocket.constructor` | `public_constructor` |
| `createKucoinSnapshotWebsockets` | `create_snapshot_websockets` |
| `KucoinSpotPublicWebsocket.subscribe` | `subscribe_spot_public` |

Each adapted note names the full target and its capability contract; provenance was
serialized using the manifest parser/writer. There are no new supporting declarations
or exemptions. No shared type or trait was changed.

Connection algorithms preserve entry-only disposal, strict code/data short circuit,
futures token-before-server versus Spot server-before-token, live ping fallback,
native UTF-16 URL composition in endpoint/token/date conversion order, private futures
endpoint capture before create-callee lookup, live listener calls, and catch/retry scope.
POST takes an operation selector, not a URL: its capability owns the whole live
`parent.xhr.post(ENDPOINTS.key)` or Spot-table call, including callee-before-table lookup,
opaque argument identity and synchronous throws. Retry owns the whole optional error
message fallback and invocation outside the source try/catch.

Async callers must poll setup immediately to match JavaScript pre-await initiation.
POST/connect return `Result<Future,E>` to separate sync throws from later rejection.
This is a declared caller/provider boundary, not a completed concrete websocket class.
Reconnect keeps the original socket while current fields/methods may change; clears ws
before close, then resets token/endpoint/private subscribed flag before invoking/awaiting
connect. Constructor orders super, complete own-field initialization, then three live
nullish option assignments. Factory uses ordinary number/Math intrinsics and propagates
NaN; whole Array.from capability owns ToLength, allocation/RangeError and mapping timing.
Spot subscription uses whole Object.keys/for-of capability (including IteratorClose),
then native UTF-16 ordinary String intrinsic prefix/replace/split/template semantics.
Concrete transport, BaseWebSocket, language-operation providers and owning classes remain
pending; arbitrary replacement of number/Math/String intrinsics is not proved here.

## SHA-bound gates

Reviewed production SHA: `c767c7b78b88b4f1c1d9de4497f6ffe065d39dd9`.
Both commands completed with exit **0** at that SHA, sequentially:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2`
- `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test socket_setup`

Test environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
At `2b32121fa63223420e2eadab8ae1ec8c8bc70da3`, the same two commands and
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
completed with exit **0**. Extractor output confirmed 7,341 symbols / 512 files,
zero manifests to update, zero added and zero orphaned symbols. The only change from
that SHA to the reviewed production SHA condensed a five-line comment to three lines;
the two cargo gates above were nevertheless refreshed at the actual resulting SHA.
The writer `parity:extract` completed with exit 0 after manifest edits, reporting zero
updates/additions/orphans. These exits are not workspace-compilation claims.

## Native assertions and finite fixture floor

All native decoded value, Rc identity, full error, literal trace and UTF-16 assertions
run before the source child, never through decode/re-encode roundtrips. The independent
floor is **116 = 76 connections + 16 reconnects + 8 constructors + 10 pools + 6 subscriptions**.
Enumeration stops at Spot-private live-endpoint, private reconnect socket replacement,
constructor index error, pool constructor error and subscription enumeration error.
Literal expectations cover token/socket identity, URL and ping values, nullish false/zero,
constructed socket indices/count, full TypeError fields/identity and Infinity RangeError
name/message with absent code/ids. Pool counts use explicit independent ceiling examples.

The source oracle executes the nine maintained bodies, actual endpoint definitions,
actual constructor field initializers and the maintained pool-cap constant. Witnesses
include disposal during token await, numeric versus string code, missing/zero-length
servers, missing endpoint, falsy ping, URL coercion changing token, current socket replacement,
logger throw caught by retry, retry throw escaping, and an xhr.post getter replacing the
endpoint with an uncoerced opaque object before lookup. Native fixtures assert that
object's identity/payload and exact selector/get/call order before source comparison.
Source fixtures use coercion-observable objects with explicit native decoded values;
some trace labels denote whole expression capabilities rather than a count of all
individual JavaScript Gets. No exhaustive getter/prototype/intrinsic census is claimed.

## Discriminating controls

All five compiling family controls were measured at
`c767c7b78b88b4f1c1d9de4497f6ffe065d39dd9` against the same focused target:

| Family / call site and mutation | Named native failure | Broken / restored exits |
| --- | --- | --- |
| Connections: `connect_futures_public`, token URL key -> wrong | `CONNECT native UTF16 URL state` | 101 / 0 |
| Reconnects: `reconnect_futures_public`, close before clearing ws | `RECONNECT native undefined before close` | 101 / 0 |
| Constructor: `public_constructor`, shardCount fallback 1 -> 0 | `CTOR native nullish shardCount` | 101 / 0 |
| Pool: `create_snapshot_websockets`, cap 380 -> 400 | `POOL native independent shard ceiling` | 101 / 0 |
| Subscriptions: `subscribe_spot_public`, ticker topic -> wrong | `SUBSCRIBE native snapshot UTF16 topics and undefined interval` | 101 / 0 |

Each mutation compiled, failed its named native assertion, was byte-restored and passed
the same focused target. Zero survivors among these five controls. This is five family
controls for nine rows, not nine individual symbol mutations. An earlier round at
`5b117d70e02b8fc091ae54a6aa9e910a625722c0` also caught/restored all five controls,
but duplicated endpoint-table stubs were found by static review and corrected afterward;
that round is not claimed as proof of the live endpoint dependency. Initial mutable-borrow
compile errors and wire integer/float presentation mismatch were fixed before that SHA;
neither is counted as a discrimination control. Static review cleared the endpoint fix
and the remaining production boundaries; the reviewer ran no cargo/tests/index.

## Census and unrun gates

Independent Python 3.11 tomllib enumeration of all 12 KuCoin manifests against the
actual parent at `5b117d70e02b8fc091ae54a6aa9e910a625722c0`, stopping at
`kucoinspot.ws-public.ts.toml`, measured 88/39/64
ported/divergent/pending -> **97/39/55**. Only the nine named statuses changed.
Workspace builds, full venue suites, host clippy, wasm clippy, full Rust-index,
CI, exchange/network tests and concrete class/runtime assembly were not run or proved.

## Delivery merge preservation and refreshed gates

Fetched and merged integration `845cd310a963e53fc50dd9180a152e034262b6b4` without
conflicts at merge SHA `0cc01020c39844b6468f86ff7d1197ff62d07c8a`.
`git diff --exit-code c767c7b78b88b4f1c1d9de4497f6ffe065d39dd9 HEAD --`
KuCoin `src`, `tests`, `scripts`, its entire manifest directory and `package.json`
completed with exit **0** at that merge. Production, native fixtures, source oracle,
all nine mappings and explicit test command survived byte-for-byte.

Shared exchange source changed in integration. Both exact scoped cargo commands above
were refreshed sequentially at `0cc01020c39844b6468f86ff7d1197ff62d07c8a`;
the check and focused 116-fixture test each completed with exit **0**. The exact
extractor check at that same merge SHA completed with exit **0**, with output showing
7,341 symbols / 512 files, zero manifests to update, zero added and zero orphaned
symbols. No old sheet is presented as freshly measured. The five family controls remain
measurements at reviewed production SHA `c767c7b78b88b4f1c1d9de4497f6ffe065d39dd9`,
whose tested source was preserved; they were not relabeled as merge-SHA control runs.

A fresh independent Python 3.11 tomllib count at the merge SHA compared all 12 KuCoin
manifests against merged integration `845cd310a963e53fc50dd9180a152e034262b6b4`,
stopping at `kucoinspot.ws-public.ts.toml`: 88/39/64 -> 97/39/55.
Exactly the nine named rows moved pending -> ported; no other status edits or divergent
changes. The subsequent receipt-only commit changes no gate inputs. Unrun gates and
concrete assembly limits listed above still apply.
