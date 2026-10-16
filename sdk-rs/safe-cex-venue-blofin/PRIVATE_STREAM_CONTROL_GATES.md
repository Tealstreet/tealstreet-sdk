# Blofin private stream control

Ten pending rows moved: **7 ported / 3 divergent**. Endpoint, disposal,
connect/open/ping, authentication, subscription and message/debounce control
bodies have concrete Rust targets. Full private facade and its three data
handlers remain pending; the public facade/listeners and exchange facade are
outside this slice.

Actual integration parent: **3cc94529835dde67ba3e6af513a233baa307c3f0**.
Tomllib census stopped after seven Blofin manifests / 125 rows: before
**28 ported / 72 divergent / 25 pending**, after **35 ported / 75 divergent /
15 pending**. This inventory does not claim behavioral completeness.

## Measured SHAs and gates

- Initial implementation/source audit: `8fa469b2b727e6556064b7c1d86e0468214f6fa1`.
- Empty-array test correction: `c2159dd46657e4096cd836f8b8c3d9daefd63be7`.
- Production/error/dispatch corrections: `b6345312575de49718b8b6f6860c35882f086364`.
- **Clean scoped gates and static review:
  `0aad614eba747657b60406b5802c1a943d08031b`.** The last commit changes the test
  task alias and canonical note citations; production diff from b634 exits 0.

Cargo commands ran serially, crate `safe-cex-venue-blofin`, jobs 2. Commands,
measured exits and raw logs are in `private-control-evidence/clean-0aad61/`:

| Gate | Exit/result |
| --- | --- |
| Scoped Cargo check, all targets | 0 |
| Crate-only host Clippy, all targets, `--no-deps -D warnings` | 0 |
| `private_stream_control` tests | 0; 10 passed |
| New source/test Rustfmt check | 0 |
| Extractor check | 0; output confirms zero updates/additions/orphans |
| WS-private source declaration coverage | 0; stopped after 1 file / 14 declarations |
| Blofin-only Rust parity audit | 1; exactly 3 inherited position-adapter findings |

The existing Rust parity binary was not rebuilt this slice. Its inherited
findings are `BlofinPositionAdapterDeps`, `map_blofin_position_history`, and `num`.
Parent source preservation diff exits 0. The scope uses production symlinks and
empty exemptions. Real qualified types/trait members were generated from source;
extractor owns TOML serialization. No fake facade target or exemption was added.

## Native proof and declared differences

`tests/private_stream_control_oracle.cjs` executes actual maintained arrow bodies,
get endpoint, dispose method and jsonParse, genuine HMAC, actual URLs and actual
DefaultStore. Base, routing, socket, signer, clock and scheduler capabilities are
injected boundaries. The configured host must implement their source semantics,
including deferred timer callbacks and retaining started tasks through completion.
Stable event-callback wiring and complete live socket/facade composition remain
unproven. Concrete timer fixtures use numeric handles, including zero; other
host-handle representations are interface capabilities, not enumerated tests.

Enumeration stopped at **85 source cases**, with fixed group floors
2 endpoint / 5 dispose / 8 connect / 6 open / 4 ping / 19 auth / 3 subscribe /
24 message / 7 throttle / 7 fetch. Native assertions cover StoreData/orders/loaded
flags, f64 bits, timer/socket/disposal state, typed task errors, request fields and
independently decoded emitted frames/payloads before wire/call-trace comparisons.
Native StoreData is not encoded or decoded for these assertions. No same-schema
roundtrip serves as a gate.

Local auth performs its prefix eagerly, sending login before onOpen's ping;
remote completion crosses an await and then reads live credentials. HMAC is
SHA256 hex text followed by base64, not base64 of binary digest. Remote errors
use `Remote WS signing unavailable`, avoiding the source auth-invalid classifier.
Plain JSON non-callable toString errors (including nested arrays) propagate typed
TypeError rejection rather than panic. Request kind/exchange/output format,
account, toSign, captured timestamp and nonce are direct native fields.
The remote capability is supplied separately because shared ExchangeOptions'
remote marker is uninhabited; no working live signer integration is claimed.

Three specific divergences:

- **MIXED onMessage:** FORCED, valid ASCII raw JSON with escaped D800 materializes
  a lone native UTF-16 unit in source; its callback asserts length1 and charCodeAt
  D800 before JSON encoding. Native String/Value cannot retain it, and skips the
  handler. The raw UTF-8 boundary does not eliminate this parsed-value conflict.
  CHOSEN, shared parser recursion budget refuses the source's depth130 array with
  native leaf42, keeping bounded recursion. Source/native witnesses retain and
  compare pre-parse track/diagnostic/log effects and explicitly assert source
  dispatch versus native refusal. Both canonical boundary anchors are in the row.
- **CHOSEN throttledFetchOrders:** source reuses its instance arrow callback;
  native creates a fresh owned closure per arm. A retained callback-identity
  witness observes source sameness and distinct native Rc callbacks. This avoids
  a cached self-owning closure cycle. Truthy old-timer cancellation and unguarded
  2000ms rearm otherwise match.
- **CHOSEN fetchThrottledOrders:** native Store owns fetched Vec/order records,
  while source retains mutable array/record aliases. Source changes a committed
  record to77 (or appends it to an empty array); native stays11/empty. This prevents
  caller mutation of committed state. Timeout resets before disposal guard;
  in-flight results still commit after disposal, current loaded flags are spread
  at commit, empty results replace old orders, and fetch/update failures are silent.

## Review and retained failures

Static sub-agent review corrected auth coercion panic and undeclared parsed-string/
depth limits. It also verified the empty-array witness and exact canonical note
citations. Clean review is bound to 0aad; reviewer ran no Cargo, oracle, controls
or live integration and made no edits. Review records are in `review.json`.

Development compile exit101 was a test's inherent/trait Store.update mismatch.
A subsequent development run had 9 passes/1 failure from an empty-array alias
assertion; both attempts are retained, not claimed as final SHA-bound gates.
At c215, check0/lint101 (production signature complexity); at b634, check0/lint101
(test task-field complexity). Neither lint-failed driver reached its test stage.
New corpus work during b634 was retained separately and the committed oracle
restored; no test stage ran with that mixed corpus. Clean 0aad results supersede
these attempts without relabeling their SHAs.

Not run this slice: dependency-inclusive host Clippy, wasm compilation/Clippy,
workspace compilation/suites, full crate suites, live HTTP/venue/socket/remote
signer integration, or CI reruns. A scoped check is not workspace compilation.
Integration ancestors are inherited, not independently reviewed by this lane;
no private lane branch was merged. Raw logs retain terminal blank lines.

## Runtime controls and merge preservation

All **11** deliberate runtime controls at
`0aad614eba747657b60406b5802c1a943d08031b` failed their named assertion with exit
**101** and passed after restoration with exit **0**. Enumeration stopped at
control **11**; no survivors or compiler-only catches. Call sites, commands,
results and raw broken/restored logs are under `private-control-evidence/controls/`.
The added signer-coercion control verifies typed rejection rather than hidden
fallback; all decoded native error/name/message assertions precede wire comparison.

Owned source/tests/crate manifest/parity mapping restoration diff exits **0**,
retained in `mutation-restoration.json`.

Integration **`6c27a13bdd9dc5645060283a5d74459405b7a0e2`** merged down at
**`f682539d5d8db3e62b1528d068a64e9dccdba35e`**. Scoped owned-file diff against
clean gate SHA0aad exits **0**. Inspected shared Rust/source-oracle files are
unchanged; package.json, SDK Cargo.lock and workspace Cargo.toml changed.
`merge-preservation.json` lists the exact scope and changed inputs. Earlier
measurements stay bound to 0aad; changed-input gates are recorded separately at
the merge SHA and never relabeled.

## Final merge-SHA gates

At **`f682539d5d8db3e62b1528d068a64e9dccdba35e`**, scoped all-targets
Cargo check, crate-only host Clippy (`--no-deps`, `-D warnings`), the named
private-stream test target (**10 passed**, **85 oracle cases**), and scoped
rustfmt all exit **0**. Extractor check exits **0**, with zero updates, additions
or orphans in its output. The source scope driver exits **0** at one file and
14 declarations; this enumeration stops there. Independent tomllib census
remains **35 ported / 75 divergent / 15 pending**, moving **10 rows** from
the original **28 / 72 / 25**.

The Blofin-only existing-binary Rust-parity check exits **1** with exactly the
three inherited position-adapter errors; the binary was not rebuilt this slice.
The scoped preservation diff exits **0**. Final static sub-agent review is
clean at this merge SHA. Completed commands, SHAs, exits and raw output are
in `private-control-evidence/post-merge-f68253/`; review binding is in
`private-control-evidence/review.json`.

The delivery commit adds only this document and raw evidence. Its scoped
source/test/crate-manifest/parity preservation check is recorded in
`private-control-evidence/packaging-preservation.json`; gates remain attached
to the measured merge SHA, while the 11 mutation controls remain attached
to their measured SHA0aad. No additional compilation claim is made for the
evidence-only commit.

## Push-time integration preservation

Integration advanced to **`9fad5d8d1bbb15b743af7af172cbcc5c69238a5a`** and
merged down at **`ca9db2d384552aa8c1a554a9def3540fa52e61a7`**. Scoped owned-file
diff from measured f682 exits **0**; inspected shared source, oracle, extractor
implementation and Cargo inputs are byte-unchanged, including dependency crate
manifests. Cargo gates were **not rerun** at ca9; their exits remain bound to f682.
The changed global manifest set received a fresh extractor check: exit **0**,
zero updates/additions/orphans. Independent tomllib census across seven Blofin
manifest files remains **35 / 75 / 15**, moving ten rows. Clean static
preservation review is bound to ca9. Records are in
`private-control-evidence/push-merge-ca9db2/` and `review.json`.

An initial packaging attempt used the system Python without tomllib and exited
**1** before running any gate or writing evidence. It was retried with Homebrew
Python3.12; this packaging failure is not a Rust or extractor gate result.
