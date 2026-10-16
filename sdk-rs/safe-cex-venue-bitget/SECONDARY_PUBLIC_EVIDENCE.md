# Concrete secondary public ticker socket

Moved **1 row pending→ported, 6→divergent**, measured with Python 3.11
`tomllib` over all eleven Bitget manifests, with unchanged 227 symbol identities.
Against latest merged integration `f3189bc39dbbb104821ef45cdcc838c99a63c174`,
the final census is **41 ported / 74 divergent / 112 pending → 42 / 80 / 105**.
Against original integration base `355683b84a7c62f3f5c9e91910c7e15438c57076`,
the initial code census was **37 / 74 / 116 → 38 / 80 / 109**.
The base has 116 pending rows, rather than the assignment's 115. This is a
manifest census, not exhaustive venue proof. Ping mapping from the separate
PR 7428 is deliberately not counted a second time.

Initial tested code SHA: **`8ebed266fce05844f1e7233d75e6ede9c5437680`**.
Its actual immediate parent is `91409bfb4078fbf19389d1bf0a6c877832d99fbd`;
initial assembly commit `c8640dff00b1fd571acda155d7e5d55ff1280ff3` branches directly
from integration `355683b84a7c62f3f5c9e91910c7e15438c57076`. Those two own slice
commits are ancestors; no other unreviewed lane branch was used as a base.
This evidence is committed after its tested code SHA; it does not claim that
later code or dependency changes inherit these exits automatically.

## What is assembled

`BitgetSecondaryPublicWebsocket::new` constructs the actual shared BaseWebSocket
with its protocol bound from construction. A real Exchange parent bridge, event
emitter, routing resolver, shared JSON parser, ticker handler and DefaultStore
carry the class operations. Native host Open and Message events go through the
core event loop; the fixture supplies in-process host I/O, not a live exchange.
The shared `has_websocket` helper tests socket presence independently of OPEN
state or first traffic, matching the source `this.ws` guard.

The newly mapped rows are class, constructor, connectAndSubscribe, onOpen,
onMarketsChanged, subscribe and onMessage. **onOpen is ported**. Six rows retain
explicit canonical CHOSEN/MIXED buckets: the shared weak parent replaces the
source strong parent; the core owns asynchronous dialing rather than installing
the wrapper synchronously; UTF-8 input/data-only topic maps exclude source
UTF-16 and executable descriptors; binary frames follow Node Buffer semantics;
errors use the shared ExchangeError ABI. A later escaped lone surrogate refuses
the entire native packet, while source commits an earlier ASCII ticker row.
Direct-entry subscriber errors retain identity metadata and the first committed
row; host-loop error reporting remains the shared core contract.

Borrowed state views cannot be rebound through a uniquely owned facade. Native
regression assertions change the index and topic map and observe actual protocol
selection and sending. Mutable facade handles were removed after review.

## Initial gates measured at 8ebed266fc

All Cargo commands were sequential, with `/opt/homebrew/opt/rustup/bin` on PATH.
Source capture used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Exit and observed output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0; finished dev profile, zero errors |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-exchange --all-targets --jobs 2` | 0; finished dev profile, zero errors; this checks the shared presence helper's crate |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test secondary_public --jobs 2 -- --include-ignored` | Baseline 0 and restored 0; six passed, zero failed/ignored, including actual-source capture |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; read output: 7341 symbols, would update zero manifests, zero added/orphaned |
| Scoped rustfmt check on new source, test and host fixture | 0 |
| `git diff --check`; restored tracked diff against tested code SHA | 0; 0 |
| Python 3.11 tomllib census | 0; eleven files, 227 matching identities, exactly seven status moves |

Final merge-down measurements are recorded below. These gates
are not workspace compilation, full venue regression,
host/wasm Clippy, wasm build, CI, live venue calls, or exhaustive JavaScript
semantics. None of those was run for this slice. The separate PR 7428 merge
repair has its own gate evidence on `feat/rust-venue-bitget-reads`.

## Behavioral floor and discrimination

The fixed independent expectation is **18 cases = three Bitget families × two
networks × three indices**. The walk stops at Bitgetinverse, testnet=true,
index=2. Each case checks native constructor state, ordered topic keys, actual
host delivery, outgoing decoded argument fields, listener replacement, ticker
fields with distinct numeric bits, counts, pong latency and binary-pong behavior
before serializing any native output for source comparison. Additional native
tests cover fallback latching, CONNECTING/CLOSED socket presence, dropped parent,
UTF-16 packet refusal, subscriber failure/partial commit and shared state views.
The resumable dial fixture separately proves an existing pending dial installs a
silent socket when released without disposal; disposal prevents that installation.

The actual source class body is captured with supplied inherited Base/host and
routing operations. This is not execution of the full TypeScript BaseWebSocket
or semantic TypeScript type checking. Numeric canonicalization for final source
comparison happens after native value assertions. Counts are fixed expectations,
not inferred from the returned artifact. This is a finite floor.

Every control below compiled, exited **101**, and failed the named native
assertion. The edits were restored byte-for-byte; the final target exited **0**
with six tests passed. There were no behavioral survivors.

| Deliberate break | Named assertion |
| --- | --- |
| Skip actual protocol open binding | `SECONDARY native open sends` |
| Shift constructor index | `SECONDARY native constructor index` |
| Delay fallback threshold to three | `SECONDARY native fallback latch` |
| Retain previous market listener | `SECONDARY native one market listener` |
| Shift added-market modulo selection | `SECONDARY native symbol order` |
| Swap product and channel fields | `SECONDARY native product type` |
| Increment message count by two | `SECONDARY native delivered message count` |
| Omit disposal before releasing existing dial | `SECONDARY cancelled dial cannot install socket` |
| Narrow socket presence to OPEN | `SECONDARY native socket presence includes non-open state` |

An earlier binding-control attempt at `91409bfb40` exited 101 on an unused
argument compiler diagnostic and was **not** counted as discrimination. Its
restored target exited 0. The corrected control above compiles and fails the
native assertion. No compile failure is being represented as behavioral proof.

## Review and remaining scope

Reviewer **`/root/account_read_review`** inspected committed blobs at
`c8640dff00`, `91409bfb40`, then final
**`8ebed266fce05844f1e7233d75e6ede9c5437680`**. The initial review found public
Rc rebinding and a non-discriminating cancellation witness; the follow-up also
found a permanently pending mock. All were fixed. Final read-only review exited
0 and reported no additional blockers. The reviewer did not run or certify
Cargo, extractor, mutation results, or source capture at the final SHA. It ran
source capture and an independent 18-case checker at earlier reviewed SHAs,
both exit 0; an optional earlier TOML check failed before checks because the
reviewer's default Python lacked tomllib. The parent census above used Python
3.11 with tomllib successfully.

No native Close event, enabled-reconnection branch, send-failure or send-reentry
case was exercised. A source witness only observes close-handler registration.
Full Bitget Exchange registration/REST assembly and primary/private socket
assembly remain pending. This slice delivers concrete secondary ticker assembly
without claiming those other classes.

## Final merged code and gates

Final tested code SHA: **`b44d208ab5476d34ae8bbc04cda6be62ae82e811`**;
actual immediate parent: `b99b86a7d5c42e2d56fe608da93ede2462f27e74`.
Merge `2a90e3fcb985362db7af853d90eebbb1f2acf9cc` has first parent
`5f10436511d5be7a01f690b244c36a8d04718089` and integration second parent
`df065d29b58fd12bdc103e2ef156c8de602663a0`. Integration already contains the
four PR 7428 rows; they are excluded from this slice's seven-row credit.
No other lane's unreviewed branch was added outside integration.

The required scoped diff of `8ebed266fc` to the merge's Bitget `src`/`tests`
exited **1**: integration added its ping/promotion changes. A narrower diff
over assembly source, test, fixture and capture initially exited **0**.
Integration changed the shared runtime clock trait: the post-merge all-targets
check at `2a90e3fcb9` exited **101** for the new fixture's obsolete clock method.
After moving its unchanged clock body, check at `b99b86a7d5` exited **101**
because the existing historical-position fixture lacked the new trait method.
Both fixtures are now adapted; this was not a byte-unchanged whole-crate merge.

All following exits were measured at final tested code SHA, after those fixes:

| Gate | Measured exit and output |
| --- | --- |
| Scoped Bitget `cargo check --all-targets --jobs 2`, same manifest/package command as above | **0**, zero errors |
| Scoped safe-cex-exchange `cargo check --all-targets --jobs 2`, same command as above | **0**, zero errors |
| Secondary test target, same command as above | Baseline **0**, restored **0**; six passed, zero failed/ignored |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test historical_positions --jobs 2 -- --include-ignored` | **0**; two passed, zero failed/ignored; unexpected monotonic-clock sentinel not invoked |
| Required `parity:extract:check` command as above | **0**; read complete output: 7341 symbols, would update zero manifests, zero added/orphaned |
| Scoped rustfmt check, including adapted history fixture | **0** |
| `git diff --check`; restored gate-input diff of crate, shared helper and parity data to tested SHA | **0**; **0** |
| Python 3.11 tomllib census against merged integration | **0**; independent expectations eleven files and 227 identities, exactly one pending→ported and six pending→divergent |

All **nine** listed controls were repeated at this final code SHA, compiled,
exited **101** and failed the same named assertions. All restored edits matched
the tested code SHA; the restored six-test target exited **0**. No survivor.
The extractor writer after conflict resolution also exited **0**, with output
reporting zero manifests updated and zero added/orphaned symbols. Conflict
resolution retained both exemption sets with no duplicate targets and used
parseManifest/serializeManifest to combine file provenance and retain
integration's ported ping row.

Reviewer `/root/account_read_review` inspected the committed merge at
`2a90e3fcb9`, then the clock adaptations at `b99b86a7d5` and final
**`b44d208ab5476d34ae8bbc04cda6be62ae82e811`**, reporting no additional blockers.
Final incremental inspection exited **0**; no final gates were run by the
reviewer. Its focused merge TOML/union check exited **0** after an initial **1**
from an incorrect ordering assumption, which it corrected to compare the actual
provenance set union. It did not certify unrelated integration work. The native
floor and limits above remain unchanged. Full venue/workspace suites, standalone
Rust invention checker, host/wasm Clippy, wasm build, live calls and CI were not run.

## Latest integration receipt

Integration advanced to **`ed7ac407c716cea8b8115aea53e466c9ac69d129`** before
the final push. Conflict-free merge/code gate SHA
**`8e134caa8246ee32767227a051b274ac7ce95bc2`** has first parent
`7bfa86d0b8ecea76fb2c57e561c1922b5bdc3c14` and that integration second parent.
Scoped `git diff --exit-code b44d208ab5 HEAD --
sdk-rs/safe-cex-venue-bitget/src sdk-rs/safe-cex-venue-bitget/tests` exited **0**.
The shared exchange source/tests also diffed **0**. Thus the nine compiling
discrimination controls remain measurements at **b44d208ab5**, preserved by
unchanged implementation and assertions; they were not rerun at this latest SHA.

The lockfile and shared numeric/crypto exports changed. Gates were rerun at
**8e134caa8246ee32767227a051b274ac7ce95bc2**:

| Gate | Measured exit |
| --- | --- |
| Exact scoped Bitget all-targets check above | **0**, zero errors |
| Exact scoped exchange all-targets check above | **0**, zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test secondary_public --test historical_positions --jobs 2 -- --include-ignored` with source environment above | **0**, six secondary and two history tests passed, zero failed/ignored |
| Exact required extractor check above | **0**, 7341 symbols, would update zero manifests, zero added/orphaned |
| Python 3.11 tomllib census against latest integration | **0**, 41/74/112 → 42/80/105, same eleven files/227 identities, one pending→ported and six pending→divergent |
| `git diff --check` | **0** |

Reviewer `/root/account_read_review` reviewed this exact merge SHA within a
focused preservation scope: checker exit **0**, no new blockers. It independently
proved Bitget source/tests/capture/mappings and shared exchange source/tests
byte-unchanged from the reviewed b44 SHA. It ran no compilation, native tests,
extractor or mutations and did not review unrelated integration changes. The
coverage floor and non-claims above still apply; evidence-only commits following
this SHA do not constitute another compilation or mutation measurement.

Final fetch/merge observed integration
**`f3189bc39dbbb104821ef45cdcc838c99a63c174`** and produced
**`a98eba6fe2ac0dc463c21675338b0340870a4dca`** (parents
`6d7185883d2a4c79332005dc70cc685f2545d2ba`, that integration SHA).
The required own-crate src/tests diff against **8e134caa82** exited **0**.
Separate diffs including owned scripts/Cargo/mappings and shared exchange,
types, numeric, crypto, store, transport, candle-cache source/manifests and
workspace Cargo/lock inputs exited **0**. Compile/test measurements above
therefore remain measurements at **8e134caa82**, with byte-unchanged inputs;
there was no additional compilation/test/mutation run at a98.
Other venues' parity metadata changed, so required extractor check was rerun at
**a98eba6fe2ac0dc463c21675338b0340870a4dca**, exit **0**: complete output says
7341 symbols, would update zero manifests, zero added/orphaned. Python 3.11
tomllib census at that SHA against latest integration also exited **0**:
eleven files/227 identities, **41/74/112 → 42/80/105**, exactly **one
pending→ported and six pending→divergent**. No new own implementation or test
change followed the reviewed preservation snapshot. The final evidence commit
only records these measured SHAs and is followed by the branch push.
