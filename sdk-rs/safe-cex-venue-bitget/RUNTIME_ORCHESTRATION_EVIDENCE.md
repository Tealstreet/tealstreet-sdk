# Bitget runtime and public subscriptions

Branch `feat/rust-bitget-runtime-orchestration`, direct integration parent
**970c8a3a41e3ff7e0da31cbf524f7d6b0e064849**. Body commit
**0fdb99b4a481735d962d73b233aa3252067c3e62** has that immediate parent.
Packet-proof commits **d427e74e3ce210316ef112e7d85d03fde141626f** and
**af61080ef831f5de8210c50f1896fcfde2c26b9a** change tests only. Final own
review/check/control SHA is af610. No separate unreviewed lane ancestor.
Incoming integration work is outside this own-slice review.

Python 3.11 tomllib census at **af61080ef831f5de8210c50f1896fcfde2c26b9a**:
**9 pending -> ported, 0 -> divergent**. **128 ported / 80 divergent / 19 pending
-> 137 / 80 / 10**. Eleven manifests and 227 unchanged `(file, kind, ts)`
identities; enumeration stopped at sorted file eleven,
`bitgetusdc.exchange.ts.toml`. Census is distinct from proof.

| TypeScript symbol | Executable body in src/runtime_orchestration.rs |
| --- | --- |
| BitgetExchange.validateAccount | validate_account |
| BitgetExchange.start | start |
| BitgetExchange.tick | tick |
| BitgetExchange#_detectAccountType | detect_account_type |
| BitgetExchange#ensureAccountTypeDetected | ensure_account_type_detected |
| BitgetExchange#resolveSymbolPositioning | resolve_symbol_positioning |
| BitgetExchange.fetchIncomeHistoryRange | fetch_income_history_range |
| BitgetPublicWebsocket.listenOHLCV | listen_ohlcv |
| BitgetPublicWebsocket.listenOrderBook | listen_order_book |

Bodies retain validation suppression and environment guidance, startup ordering
and disposal boundaries, public/external modes, periodic balance/order cadence,
virtual-order merge and delayed live tick lookup, sticky probe flags and terminal
versus transient errors, inflight finally cleanup, original positioning values
and TTL/nonempty-field caches, income callback/options/filtering and incomplete
range warning. Public listeners retain owned native parsers, 100ms polling,
handler arrays (including empty teardown keys), per-symbol topic counts, parser
identity filtering, callback values, book alias/delta state and different candle
versus book cleanup/send order. Candle volume reads column 6. Book teardown
clears asks then bids, before canceling the polling timer; candle cancels first.

Three exemptions were generated from the actual RuntimeOrchestrationHost trait,
owned function factory and one-element iterator-destructuring method. Existing
REST/placement/preparation contracts supply native receiver/callee owners,
fields/arrays/iteration/imported operations, independently owned launch and
promise adoption. Clones retain the same receiver/runtime. Callback functions
use shared Fn access rather than holding a Rust mutable callback borrow. Native
first destructuring includes IteratorClose. Concrete Exchange/class/HTTP/socket
assembly, API creation and inverse open-position reads remain pending. No shared
type, shared trait, dependency or lockfile was changed by this slice.

## Native and actual-source floors

Nine tests assert decoded native variants/fields, ordered objects/arrays, f64 bits,
UTF-16, independent counts, original balance/market/order/income/cache/promise/
callback/book handles, flag restoration, loaded-state boundaries, endpoint and
page arguments, fixed candle columns, snapshot/delta book values, native packet
objects before stringify, timer delay and cleanup order. Expectations are literal
fixture recipes, not counts taken from outputs. Every native check for every
fixture in a group finishes before that group's first wire/source projection.
Serialized send text is compared only afterward; the native gate retains and
asserts the original pre-stringify packet op/args/instType/channel/instId.
There is no decode/re-encode witness.

| Group | Independent case floor | Last iteration |
| --- | --- | --- |
| Validation | 6 | 5, empty-message default |
| Startup | 10 | 9, market fetch failure |
| Tick | 10 | 9, emitter failure |
| Account probe | 8 | 7, fallback logger failure |
| Detection dedup | 6 | 5, finally setter failure |
| Symbol positioning | 8 | 7, initial position-read failure |
| Income range | 7 | 6, warning logger failure |
| Candle subscription | 7 | 6, send failure |
| Book subscription | 8 | 7, snapshot then update |

Total **70 cases**, a bounded floor. Source captures execute nine actual
initializers/method bodies, actual endpoint/interval/TTL/bill constants, OrderType
and LogSeverity, real probe success/message/environment helpers, and actual
book update/binary-search/total helpers. Receiver/HTTP/store/paging/clock/loop/
timer collaborators are controlled dependencies; source collaborator calls assert
their native receiver owner. Native array methods assert decoded selectors/owners;
other receiver/import/intrinsic bindings remain supplied contracts. ISO evidence
covers only epoch 0 and 1000ms; it is not general calendar proof. Promise fixtures
yield and settle in a bounded polling executor. Ordinary owned values, fixed
numeric/parser inputs and finite timer/callback sequences do not certify arbitrary
getters/proxies/prototypes/coercions, changing intrinsics, arbitrary scheduling,
concurrent dedup, multi-await, cancellation or runtime lifetime/ownership domains.

Precommit attempts: first test compile exited **101** for a duplicated harness
trait implementation; it was removed (not a named mutation gate). Next runnable
run exited **101**, eight groups passed, source probe comparison found a fixture
binding returning the symbolic endpoint name rather than its URL. Corrected the
binding and added an independent native URL assertion. Next run exited **0**,
nine passed. Callback storage then changed to shared Fn, and review-driven native
packet assertions were added at the committed SHAs above. Restoration gates below
cover the final committed fixture, not merely these earlier passes.

## Own sub-agent review citation

`/root/account_read_review` reviewed all nine committed bodies at
**0fdb99b4a481735d962d73b233aa3252067c3e62**, verifying the direct parent above.
No production parity/mapping blocker within declared contract scope. It found
two P2 proof gaps: no original packet object assertion, then early serialized
packet-text equality before later native assertions. Both were fixed in the two
test-only commits above. Final clean own-review citation is
**af61080ef831f5de8210c50f1896fcfde2c26b9a**. Packet fields/order and independent
scene counts are now native gates: zero early/disposed, one shared/send-error,
two completed final teardowns. All native group checks precede wire comparisons.

Reviewer committed reads **0**; TOML exactly nine transitions/three actual distinct
helpers and direct-parent check **0**; independent committed-source probe floor
eight node/Python **0**, testing endpoint/results/error identity/log counts/finally
flags. Those measurements belong to **0fdb**, not af610; seven TS inputs were
byte-verified, loader/root rebased for node -e. Probe enumeration stopped at
index 7, fallback logger failure. At final **af610**, committed delta inspection
and standalone production/script/manifest/exemption preservation diff versus
0fdb each exited **0**. No remaining blocker; no edits. Reviewer did not rerun
runtime gates at af610: no Cargo/native tests, source script, extractor, compiling
controls, host/wasm clippy, workspace/CI. Broad binding/concurrency contracts
remain unproven as stated above.

## Completed gate exits

Cargo uses `/opt/homebrew/opt/rustup/bin` on PATH, two jobs, one Cargo at a time.
Native/source tests use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Scoped command is `cargo check --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-bitget --all-targets --jobs 2`.

| Exact measured SHA | Gate | Exit |
| --- | --- | --- |
| 0fdb99b4a481735d962d73b233aa3252067c3e62 | Scoped all-target check | 0 |
| 0fdb99b4a481735d962d73b233aa3252067c3e62 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract | 0; output read zero updated/added/orphaned |
| d427e74e3ce210316ef112e7d85d03fde141626f | Scoped all-target check | 0 |
| d427e74e3ce210316ef112e7d85d03fde141626f | same node/yarn parity:extract:check | 0; output read zero would-update/added/orphaned |
| af61080ef831f5de8210c50f1896fcfde2c26b9a | Scoped all-target check | 0, Finished dev |
| af61080ef831f5de8210c50f1896fcfde2c26b9a | Python 3.11 tomllib census | 0, nine transitions above |

Compiling mutation/restoration and integration results are appended only after
completion. The initial uncommitted lib-only preflight exited 0; it is not claimed
as a committed all-target gate. Test-only review fixes leave extractor source/
manifest inputs byte-unchanged; merged metadata checks are recorded separately.

Not run for this slice: full Bitget crate suite, workspace/monorepo builds,
host clippy, wasm clippy, CI or live-venue tests. Previous slice full-suite exits
are not new runtime gates. The remaining ten pending and eighty divergent rows
are not certified by this slice. Concrete assembly remains pending.

Initial validation-restoration mutation at af610 exited **101** without compiling:
replacing the original saved value left `previous` unused under deny(warnings).
The runner restored exact source bytes and stopped with a non-gate diagnostic.
No test assertion ran; this is not a passing discriminator or a mutation
survivor. A revised control uses the saved value while restoring an incorrect
transformed value; its compiling result is recorded separately below.

## Compiling controls and restoration

At exact **af61080ef831f5de8210c50f1896fcfde2c26b9a**, nine one-at-a-time
controls compiled and each exited **101** at a named native assertion. Every
control restored exact original source before the next; no compiling survivor.

| Control | Exact selected test | Named native assertion |
| --- | --- | --- |
| validation-restore | `native_validate_then_source` | RUN native numeric validation suppression restored exactly: 8 != 7 |
| startup-markets | `native_start_then_source` | RUN native scalar startup market loaded flag: false != true |
| tick-cadence | `native_tick_then_source` | RUN native original reference tick balance alias |
| probe-detected | `native_detect_then_source` | RUN native scalar probe finally detected flag: false != true |
| ensure-finally | `native_ensure_then_source` | RUN native scalar detection finally clears inflight: undefined != null |
| position-margin | `native_resolve_then_source` | assertion `left == right` failed: RUN native UTF16 native symbol positioning.marginType |
| income-filter | `native_income_then_source` | assertion `left == right` failed: RUN native independent array count income independent selected rows |
| candle-volume | `native_candle_then_source` | RUN native numeric native candle columns.volume: 999 != 6 |
| book-snapshot | `native_book_then_source` | assertion `left == right` failed: RUN native independent array count native snapshot levels.bids |

Control enumeration stopped at index 8, book snapshot guard. The revised
validation control restored saved suppression + 1 (8 instead of independent
expected 7) and compiled; it supersedes only the noncompiling diagnostic above.
No exhaustive mutation coverage is claimed. After exact restoration,
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
--test runtime_orchestration --jobs 2` with the TS environment completed at
af610 with exit **0**, nine passed, 70-case floor. Selected controls used this
command with the named test and `-- --exact`. Own src/tests/scripts restoration
diff, edition-2024 rustfmt on the two new Rust files, oracle node syntax and
whitespace each independently exited **0** after restoration.

## Integration merge and preservation

Merged latest observed integration **f364cee096ff92584c157cad8b96f12b92cf3a09** down;
exact merge SHA **eb803eaf6519d371ad784c6c62f9792311a835fe**. Sole conflict was parity/exempt.toml.
Resolution used the entire stage-3 integration file plus three generated actual
helper records. Tomllib verified all 1,257 integration records retained in order,
all own records retained and unique Rust symbols; exit **0**. No CLAUDE conflict
or unreconciled file. No separate unreviewed lane ancestor; incoming integration
work is outside the own-slice review.

At the exact merge SHA: scoped git diff --exit-code versus own gate SHA
**af61080ef831f5de8210c50f1896fcfde2c26b9a** over Bitget src/tests/scripts
exited **0**, byte-identical. Wider recursive Cargo/Rust/TS/package inputs
excluding Cargo.lock diff **0**; including the lock diff **1**, disclosed.
Only safe-cex-lighter-sdk 0.1.0 changed in Cargo.lock. Independent tomllib
key/version/record comparison and traversal proved all **481** Bitget-reachable
records/dependency lists identical, Lighter SDK unreachable; exit **0**. BFS
stopped at index **480**, ucd-trie 0.1.7 (crates.io). This is reachable dependency
preservation, not whole-lock byte identity or workspace/feature compilation.

Independent tomllib census against merged integration exited **0**, again nine
pending -> ported, zero divergent; eleven files / 227 unchanged identities,
128/80/19 -> 137/80/10; stopped at sorted file eleven named above. Cargo and
compiling controls were not repeated after merge: their actual exits remain at
af610, whose source/test/script/build/TS and reachable dependency inputs are
preserved. Merged metadata measurements are recorded separately on completion.

At exact merge **eb803eaf6519d371ad784c6c62f9792311a835fe**, fresh writer
and checker each completed with exit **0**; output read zero updated/would-update,
added or orphaned symbols. A broader evidence-document commit-range whitespace
diagnostic initially exited **2** for one trailing space; it was trimmed and
the base-to-working-tree documentation check exited **0**. This evidence-only
commit introduces no new Cargo measurement. Final own byte checks precede push.

Final pre-push fetch advanced integration to **f17498f6e1e8b8e8b93c75bc11f2535b2b4bf8eb**;
merged it down, exact merge **a00642d8a925d61b17860bcd4a641e213d47e08e**. The exemption conflict
again retained the entire stage-3 file plus three own records; independent
tomllib preserved all 1,267 integration records/order and three actual helpers
with unique Rust targets, exit **0**. No CLAUDE conflict.

At this exact final merge SHA: scoped own src/tests/scripts diff versus af610
**0**; recursive Rust/TS/build/package inputs excluding Cargo.lock diff **0**;
lock tomllib record/reachable traversal proof **0**, still solely unrelated
safe-cex-lighter-sdk changed and all 481 reachable records identical, stopped
at BFS index 480 as above. The wider lock-byte difference remains disclosed.
Independent tomllib census **0**, nine pending -> ported / zero divergent,
128/80/19 -> 137/80/10, eleven manifests / 227 unchanged identities, stopped
at sorted file eleven. Cargo/control exits remain measured at preserved af610;
no rerun after either preserved-input merge and no incoming integration review
claim. Final writer and checker each completed with **0**, output read zero
updated/would-update, added or orphaned. Own byte restoration, Rust formatting,
oracle syntax and working/base documentation whitespace checks each **0**.
The following evidence-only commit introduces no new Cargo gate SHA.

## PR 7766 conflict repair

PR 7766 was still open/conflicting, not merged. This repair preserves the
existing **nine pending -> ported, zero divergent** rows; it adds **zero new
ports** beyond that already reported slice.

Merged integration **51a76599506b631dbe122bcc00a259e4a8aeb259** down into the
existing feature branch. Exact merge SHA
**7fa9d25328114fb32895a491847a106eef2fd170**, with first parent
**9f52d889d722e9479bf1c7e7312b99851225b548**. Sole conflict was parity/exempt.toml.
Resolution retained the entire stage-3 integration file plus exactly three own
reviewed helper records. Tomllib asserted all 1,269 integration records retained
in order, all three helpers retained and 1,272 unique Rust symbols; exit **0**.
No CLAUDE conflict or unreconciled file. No separate unreviewed lane ancestor.
Incoming integration code is outside the own-slice review.

At exact committed **7fa9d25328114fb32895a491847a106eef2fd170**, standalone
own src/tests/scripts diff versus reviewed af610 exited **0**, byte-identical.
Independent tomllib census against integration 51 exited **0**, nine pending
-> ported, zero divergent, eleven files / 227 unchanged identities,
128/80/19 -> 137/80/10, stopped at sorted file eleven named above.

`/root/account_read_review` reviewed exact committed **7fa9d25328114fb32895a491847a106eef2fd170**.
Reviewer verified both parents; standalone committed source/test/script
preservation versus clean af610 **0**; parsed full integration exemption
records plus exactly three actual reviewed helpers/unique Rust symbols **0**;
exact nine mappings and all unrelated Bitget rows/file metadata preserved **0**;
only eight authorized own slice/evidence/manifest/exemption/lib files changed
against integration **0**. No blocking resolution issue or edits. Reviewer
did not run Cargo/native/source/extractor/control/clippy/wasm/workspace/CI
gates at the repair SHA. Earlier source-probe measurements remain at 0fdb.

The next-slice census must not duplicate this PR: its nine rows are **seven
exchange methods plus two public listeners**. The repaired branch has only
**two exchange.ts pending rows**, the class and constructor. Remaining ten
pending rows are API creation (one), exchange class/constructor (two), private
websocket class (one), public websocket class (one), inverse class/constructor/
open-position read (three), and USDC class/constructor (two). This is census,
not proof of any remaining implementation.

Fresh scoped all-target check at the committed repair SHA exited **0**, Finished
dev. Fresh writer **0**, output read zero updated/added/orphaned. Fresh checker
**0**, output read zero would-update/added/orphaned. Cargo remains sequential,
two jobs; full scoped Bitget suite completion is recorded below only after it
finishes. This repair does not reuse the previous native-only run as a fresh
full-suite gate.

At confirmed committed **7fa9d25328114fb32895a491847a106eef2fd170**, fresh
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2
-- --include-ignored` with the TS environment completed with exit **0**.
All emitted test-result summaries passed, including the nine runtime tests;
last integration binary types passed four, then the doc-test floor was zero
(not a behavioral witness). Process completion was observed and HEAD consistency
asserted before and after both Cargo gates. PATH prefixed Rustup and Homebrew;
no concurrent Cargo. This is the full scoped Bitget crate command, not workspace
compilation, clippy, wasm or CI.

The original nine compiling controls were not rerun during this metadata repair;
their actual negatives/restored positive remain attached to unchanged af610.
Workspace builds, host/wasm clippy, CI and live tests remain unrun. Concrete
class/HTTP/socket assembly and the remaining ten rows remain pending. This
evidence-only commit does not introduce a new Cargo gate SHA.

Integration advanced during the first full suite. Merged latest observed
**9c663a908f40c2955e27b1ce58f97fb5c08fb331** cleanly, exact merge
**11f0ac01e669ce68fb72913097eca02ced048bc0**. Bitget src/tests/scripts versus
reviewed af610 still diff **0**, but the shared Exchange websocket type gained
a before-close callback field; Cargo.lock also changed. These build inputs are
not called byte-identical. Fresh scoped all-target Cargo check at confirmed
committed 11f0ac01e6 completed with **0**, Finished dev. Fresh writer and checker
at the same SHA each completed **0**, output read zero updated/would-update,
added or orphaned. Independent tomllib census against merged 9c663a908f exited
**0**, again preserving nine pending -> ported / zero divergent, 137/80/10 over
eleven manifests / 227 unchanged identities, stopped at file eleven above.
The second fresh full-suite process completion is recorded below. Reviewer
7fa9d25328 does not certify the incoming shared Exchange code.


## Push-first conflict repair, measured dc1833e2fe

The previous full scoped Bitget suite at committed
**11f0ac01e669ce68fb72913097eca02ced048bc0** completed with exit **0**,
observed before starting the following Cargo gates. Its result is not assigned
to either later merge SHA.

Following the user's explicit push-before-gates ordering, merged incoming
integration and immediately pushed **9e86a9ebe9d532e955335bea5d1ee2eccfe58811**.
Integration advanced again; merged
**234eaea814b2108fede77c0765a7368ada269523** and immediately pushed
**dc1833e2fed19754dd1a5a59b31cda6094bd8284** (push exit **0**). No new Cargo
or test gate ran before either push. No new gates are claimed for 9e86.
The second merge parents are 9e86a9ebe9 and 234eaea814. No unreconciled
file remained. The sole conflict was parity/exempt.toml: retained all 1,377
incoming records plus exactly three reviewed own real helper bindings,
1,380 unique Rust symbols. No separate unreviewed lane ancestry was introduced
outside incoming integration; unrelated integration changes were not reviewed.

At exact pushed **dc1833e2fed19754dd1a5a59b31cda6094bd8284**, freshly ran:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
  --all-targets --jobs 2`: exit **0**, observed Finished dev.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
  --jobs 2 -- --include-ignored`: exit **0**, observed process completion.
  Set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
  All reported test-result summaries passed, including the nine runtime tests;
  the last integration binary was types (four tests), followed by zero doc tests.
  Cargo commands ran sequentially, with the exact HEAD checked before and after.
- Extractor writer and checker: both exit **0**. Read their output: zero
  updated/would-update manifests, zero added and orphaned symbols.
- Standalone scoped `git diff --exit-code` of Bitget src/tests/scripts versus
  reviewed af61080ef831f5de8210c50f1896fcfde2c26b9a: exit **0**, byte-identical.
- Independent tomllib census against integration parent 234eaea814: exit **0**,
  exactly **9 pending -> ported, 0 -> divergent**; no additional rows moved
  by this merge repair. Totals 128/80/19 -> 137/80/10 over 227 unchanged row
  identities in eleven manifests. Stopped at sorted file eleven,
  `bitgetusdc.exchange.ts.toml`; this enumeration is a venue census floor.

Sub-agent review citation: `/root/account_read_review`, exact committed
**dc1833e2fed19754dd1a5a59b31cda6094bd8284**. Read-only checks exited **0**:
parents, unchanged reviewed production/tests/capture, all incoming exemptions
plus exactly three own bindings, exact nine mappings and all other Bitget row
metadata. No blocking resolution issue. Reviewer ran no Cargo, tests, extractor,
controls, clippy, wasm, workspace or CI gates and did not independently check
push state. Incoming unrelated integration code is outside this review.

Original nine compiling negative controls and restored positive remain measured
at unchanged af610; not rerun for this merge repair. Workspace compilation,
host clippy, wasm clippy, CI and live exchange tests were not run. The evidence
commit itself does not change the measured Cargo SHA above. Remaining ten
pending rows are the assembly/API/inverse-read rows enumerated earlier;
seven exchange methods already ported here must not be duplicated next slice.
