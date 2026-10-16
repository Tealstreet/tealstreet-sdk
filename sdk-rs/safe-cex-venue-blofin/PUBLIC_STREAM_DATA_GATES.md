# Blofin remaining public-stream bodies

Original parent: **7600c3e6ecfb331029421876f9d41adc40813c2d**.
Branch: `feat/rust-blofin-public-stream-data`. `.aimux-next.md` was absent;
tracked tree was clean, with only the inherited untracked brief/priority files.
No CLAUDE.md or agent-memory reread. Integration
**c749a66602fb72a64210c7f4fad4637c73fde943** merged down at
**e45dc4351554066030e76d54d39a13079e858343**. Scoped source, tests, crate
manifest, parity mappings and maintained Blofin source preservation diff from
**d5f55bf7a717333510ae68fe3d4cd81d9e5f1b71** exits **0**.
Integration ancestors are inherited, not individually reviewed by this lane;
no private lane branch was merged. User opens the PR; this lane only pushes.

Independent tomllib census at e45 across **7** Blofin manifest files:
**35 ported / 75 divergent / 15 pending → 37 / 81 / 7**.
**8 rows moved: 2 ported, 6 divergent**. This enumeration stops at seven files.
All public-websocket rows now have real Rust targets. Remaining seven rows are
private facade + three topic handlers, exchange facade + two forwarding listeners.
The venue is not claimed clear.

## Measured gates at e45

| Gate | Exit | Measured scope |
| --- | --- | --- |
| Cargo check | 0 | safe-cex-venue-blofin, all targets, jobs2 |
| Host Clippy | 0 | same crate, all targets, jobs2, --no-deps, -D warnings |
| Oracle test target | 0 | public_stream_data: **11 passed**, **93 source cases** |
| rustfmt check | 0 | new Rust source and test only |
| Extractor check | 0 | output: zero updates, additions and orphans |
| Source coverage | 0 | canonical extractor: **1 source file / 18 declarations** |
| Blofin Rust-parity | 1 | exactly three unchanged inherited position-adapter errors |

Commands, SHAs, exits and raw output: `public-data-evidence/gates-e45dc4/`.
Oracle environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The Rust-parity binary was reused, not rebuilt. No new exemptions.
Counts come from independent literal expectations and parent-manifest comparison.
Native Store/ticker/Candle/Book fields, numeric bits, shared payloads, live handler
arrays, retained roots, timer delays and typed errors are asserted before decoded
frame/wire trace comparisons; there is no decode/re-encode proof.
Enumeration stops at the listed 93 source cases and explicit native lifetime/Vec
probes. No completeness claim for other host implementations or inputs.

## Row behavior and witnessed differences

- **PORTED constructor:** inherited base first; any-message keepalive field,
  default topic/handler state, retained base-close callback before watchdog.
- **PORTED dispose:** truthy timer cancellation/reset, markets listener removal,
  then base disposal; clear/off/base failures and zero timer ordering preserved.
- **CHOSEN facade:** default callbacks use Weak to avoid owning their facade in
  an Rc cycle. Source retained ticker callback remains callable (last999);
  dropping native facade leaves its retained callback a no-op (last100).
  Registered listener closures retain host/model state rather than the facade.
- **MIXED onMessage:** source parses ASCII-escaped lone UTF16 D800 while native
  serde Value refuses it (**FORCED**, canonical manifest boundary anchors).
  Native parser's depth budget refuses depth130/leaf42 that source dispatches
  (**CHOSEN** bounded recursion). Both refusals have direct source-native witnesses
  before JSON encoding; no silently omitted callback comparison. Common routing
  retains live handler-array append, shared mutable JSON and broadcast/exact order.
- **CHOSEN ticker:** owned row snapshot avoids a payload guard across Store calls.
  A source getter mutates bidPrice to88 before patch construction; native snapshot
  patches99. Later custom callbacks observe mutated shared input in both runtimes.
- **CHOSEN funding:** same deliberate row ownership; source getter's rate0.0002
  differs from native snapshot0.0001, asserted as numeric bits. FundingTime remains
  in milliseconds. The distinction has its own fixture and note.
- **CHOSEN OHLCV:** owned options remain BTC after caller changes source opts.symbol
  to missing during loading retry. Source gets no handlers, native subscribes BTC.
  Candle root handles preserve callback mutations, identity and reentry; duplicate
  suppression, fallback/volume order, invalid-number/error order, median bridge,
  retry/cleanup behavior and string book indexing are otherwise witnessed.
- **CHOSEN book:** nested native Vec clones are owned while source bids array aliases
  see a books5 overwrite99→97. Shared book ROOT aliases update and clear in both
  runtimes. Sequence gap/reset/send order, zero skip before price coercion, string
  iterator, incremental partial writes, totals, retained tails, callback cleanup
  and send reentry are witnessed.

Final static sub-agent review is clean at **e45** (`public-data-evidence/review.json`).
Reviewer ran no Cargo, oracle or controls and did not review untracked evidence.

## Earlier failures and limits

At **681252eb7ed0982835d35c71b7b7b8e36805eb3d**: check0, Clippy101 for one
needless borrow. Tests and fmt were not reached by that driver. Extractor0,
coverage0, census8 moved; the Rust-parity driver exited1 on missing exact target
names in class support notes. Its subprocess exit was not persisted and is not
claimed. Raw diagnostics retained:18 status diagnostics +3 inherited inventions.
Notes now name the exact computed targets; no gate or target was weakened.

At **a52f72871c847adfcd0a5b89f2db51fc1be34db3**: check0, Clippy101 for a
collapsible test if; tests/fmt not reached. Development compile/oracle failures
and earlier78/87-case passes are retained as dirty development, not final gates.
The gate at e45 supersedes these measurements without relabeling their SHAs.
One evidence-copy packaging attempt exited1 on a Python syntax typo, then was
corrected; no production file or gate was affected.

Not run: workspace builds/suites, full crate suite, dependency-inclusive host
Clippy, wasm compilation/Clippy, live HTTP/socket/venue integration or CI reruns.
A typed host facade and stubbed base capabilities are not a live network proof.

## Control harness correction

Initial constructor control at e45 exited101 before runtime: skipping the base
callback made its field unread and -Dwarnings rejected it. Restoration exited0
(one named test passed). This is **not runtime discrimination** and is retained in
`public-data-evidence/compiler-only-control-e45dc4/`, with caught=false.
The corrected control still reads the field while skipping the call, so the
assertion can run. Harness-only commit
**a9b0b4a2be85aa586e602bf4af866907f282ea0d** leaves production source and
oracle/test inputs byte-unchanged from e45 (scoped diff exit0). Runtime control
results remain attached to their own measured SHA rather than e45.

## Completed runtime discrimination

At **a9b0b4a2be85aa586e602bf4af866907f282ea0d**, the corrected constructor
mutation failed `native_constructor_close_capture` at runtime with exit**101**,
then passed after restoration with exit**0**. Enumeration stopped after this
completed mutation; mutation2(dispose) compilation was interrupted (driver130),
with no mutation2 gate exit claimed. Source restoration diff exits0. Raw records
and interruption are retained under `serial-control-floor-a9b0b4/`.

At **28933ede42d592d1566d307d5c3e76687f7ff604**, **one compound control**
applies ten deliberate production changes. All **11 named tests FAILED**
at runtime, exit**101**, then all **11 passed** after restoration, exit**0**.
No surviving named tests. This proves named-test discrimination under the
compound changes; **independent detection of each mutation is unmeasured**.
Enumeration stopped at compound iteration**1**, not ten independent controls.
`compound-28933e/results.json` and raw logs retain commands, call sites, changes
and named outcomes. Constructor's independent result is a separate claim.
Final scoped restoration/preservation diff exits**0**, recorded in
`push-merge-preservation.json`; earlier compilation/gate exits remain bound to e45.

## Push-time merge and evidence delivery

Integration **51e9d74873a1c863a42569e7dfeb95cbc2da40bd** merged down at
**fb8f2c23f6a37cd1c1ae94956830f47698319f62**. Owned behavior inputs match e45
(scoped diff0); control runner matches289(scoped diff0). Inspected shared Rust,
maintained source/utility/base/config/oracle and Cargo inputs are byte-unchanged.
package.json only adds an unrelated Kucoin test target. No Cargo rerun at fb8;
Cargo exits remain attached to e45. Refreshed extractor0(textzero updates/additions/
orphans), source coverage0(1file18declarations), census37/81/7(8moved), and scoped
Rust-parity1(only3inherited errors) are bound to fb8 in `push-source-fb8f2c/`.
Clean sub-agent preservation review is bound to fb8.

Delivery adds only this document and raw evidence; compilation is not relabeled
as occurring at the evidence-only commit. The post-commit scoped diff confirms
production/test/oracle/crate configuration/parity bytes still match measured e45.
