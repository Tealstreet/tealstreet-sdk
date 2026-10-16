# Polymarket websocket connection, dispatch and asset subscription bodies

**8 pending → ported; 0 pending → divergent.** Triple **58 / 18 / 65** (ported / divergent / pending), total141. [Census](evidence/ws-dispatch/census.json) compares all seven manifests to actual requested integration parent **`11bafd67b4950c2120105f37929798edd9c9c032`**, with an independently specified eight-symbol set. Fresh branch `feat/polymarket-ws-dispatch-11bafd` begins directly at that reviewed integration head; no private unreviewed lane commits are ancestors. Census is distinct from runtime proof.

## Bodies and boundaries

Eight real individually mapped functions in src/ws_dispatch.rs: public/private connectAndSubscribe; public/private onMessage; public subscribeToAsset, unsubscribeFromAsset, subscribeToOrderBook (including its returned cleanup); public #handleLastTradePrice. Auxiliary manifest targets are real receiver/runtime trait declarations, methods, associated types or native anonymous operation/ticker records. The official venue-only checker validates all off-pending targets without exemptions. Full websocket classes, constructors, concrete hosts and composition remain pending.

Connection retains credentials-before-disposal order on the private receiver, one disposal/socket guard, info logging before URL evaluation, current socket/listener reads in open/message/close/error order, optional error.message fallback and abstract ToString failures. A listener getter may replace the socket: its already-captured receiver is retained for that registration and later registrations read the new socket. Reentrant logging changes later URL reads without adding another guard.

Message handling tracks reception before reading data, uses separate first PONG and second JSON reads, preserves the imported original jsonParse result and private authentication/error precedence, rereads error for log fallback, and retains original data/error identities. Truthy handlers record stats before capturing array length; each callback reads the captured array's current slot and skips deleted holes. Stats may append to that array and replace the registry: dispatch still uses the original array, including the newly appended handler. Truthy inherited/invalid handler entries fail after stats, rather than becoming absent.

Asset guards, Set mutations, connected reads and native operation fields retain source order, including mutations before throwing sends. Book subscription stores the original callback identity and invokes the current child subscription method. The returned cleanup executes repeatedly, rereads the current callback registry on each call, preserves callbacks belonging to others and unsubscribes only after the current set becomes empty. Last-trade handling preserves symbol lookup before price conversion, parseFloat before the falsy-symbol/finite guard, native token selector/id/symbol identities and last Number bits.

Runtime bindings provide ECMAScript property/ToPropertyKey reads, truthiness, strict string equality, abstract ToString (Symbol throws), original JSON parser, ordinary Map/Set/Array/socket operations and stringify/send. These bodies do not implement those general runtime intrinsics or a complete client. UTF-16 values retain lone surrogates; replacement occurs only inside parseFloat, where either a lone surrogate or U+FFFD terminates a numeric prefix. Arbitrary object/prototype/parser/encoder implementations, concrete network, timer progression and full client wiring are outside this slice.

## Gates attached to SHA

All final scoped exits and controls were measured after the last merge-down at **`f454364be49323149a468d6745437653937b4100`**, including integration **`85f3b9e596582fce3738807c4d9b7032c5b5a802`**. [gates.json](evidence/ws-dispatch/gates.json) records exact commands and oracle environment; raw logs are tracked. Cargo commands ran sequentially.

| Gate | Measured result |
| --- | --- |
| Scoped Cargo check, Polymarket, all targets, locked, jobs2 | 0 |
| Scoped Polymarket tests, locked, jobs2 | 0; 60 passed, zero failed; doc tests zero cases |
| Scoped Cargo fmt check | 0 |
| Official venue-only invention/status checker | 0; 141 rows, 170 public Rust items; zero exemptions |
| Actual Yarn parity:extract:check | 0; inspected output: would update0 manifests, zero added/orphaned symbols |
| Eight isolated production controls | 0; shared nine-test baseline0; each mutation101, exact restored-test0 |

Writer parity:extract was run after manifest updates: output reported updated0 manifests, zero added/orphaned symbols. Manifest arrays were edited through the extractor parse/serialize API. Dirty development drafts failed compilation101 on an ambiguous f64 expectation and an accidentally renamed test variable, then runtime101 on two fixture expectation/binding errors. These drafts were corrected before committed gates; their logs were overwritten and are not retained as gate evidence or claimed to cover this SHA. The last corrected dirty focused run passed all nine tests0.

The earlier committed attempt at `905319829729dfb62088d791fcad5af82a6e6797` measured all five scoped gates0, then the control driver exited1 at control2/private.connectAndSubscribe. The mutation compiled and failed runtime101 at **independent replay count**, restored0, but the driver had expected the later native dispatch trace assertion, so its summary records caught=false. This was a wrong expected assertion name, not a mutation survivor or compile-only proof. Control1 had runtime0/101/0; controls3–8 had not run. [Initial attempt](evidence/ws-dispatch/initial-attempt/gates.json), summary, driver and raw logs retain the exact two-control floor. The script was corrected to match that first named native replay-count assertion, with row/scenario context added to the test message. At `70d8f9abaa6802ab7375d278b3d581a6594ae0ef`, all five scoped gates0 passed again ([intermediate gates](evidence/ws-dispatch/intermediate-gates/gates.json)); controls did not run there. Integration then changed shared transport source/Cargo.lock, so all five scoped gates and eight controls ran again at `a8f2f4dcb2cc915ac7de17fd3433fb4861be10dc`, including integration `4c958de5469ba6ff2ea5ee3be7447cce5abb07c4`. That successful earlier run is retained in [pre-final merge gates](evidence/ws-dispatch/pre-final-merge-gates/gates.json). The final merge included shared exchange dependency changes, so those scoped/control exits were not extended: all five gates and eight controls were rerun at the final SHA above. [Pre-regate receipt](evidence/ws-dispatch/pre-regate-merge.json) retains own src/tests diff0 and aggregate input diff1 plus the actual dependency diff.

## Native proof and mutation controls

The oracle extracts original source AST arrow bodies, original WS_MESSAGE_TYPES/log severities and the original imported jsonParse declaration. Original arrows capture the mock receiver through an ordinary function scope; source body text is not rewritten. Source captures native fields before JSON output. Native tests directly inspect receiver state, event order, UTF-16 values, callback/opaque object/error identities and Number bits. Prepared source operation keys are independently asserted to be exactly assets_ids/operation before stringify; Rust AssetOperation fields are inspected directly. No native wire comparison, roundtrip or re-encoding is used.

Independent expected trace/state/replay counts are specified by scenario, never computed from the source artifact under test. Last-trade source selector/id and native opaque selector/id share the original token; native fields are asserted directly before encoding. Handler data changes in one callback are visible to later callbacks. Generated Symbol and invalid-handler TypeErrors are distinguished from original injected host errors; injected errors retain original identity.

| Isolated production fault | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| public.connectAndSubscribe: omit existing-socket guard | native dispatch trace:public.connectAndSubscribe | 0 / 101 / 0 |
| private.connectAndSubscribe: invert credentials guard | independent replay count:private.connectAndSubscribe index0 | 0 / 101 / 0 |
| public.onMessage, shared route: capture length before stats | native dispatch trace:public.onMessage index10 | 0 / 101 / 0 |
| private.onMessage: change authentication refresh trigger | native refresh trigger | 0 / 101 / 0 |
| public.subscribeToAsset: invert duplicate guard | native dispatch trace:public.subscribeToAsset | 0 / 101 / 0 |
| public.unsubscribeFromAsset: invert membership guard | native dispatch trace:public.unsubscribeFromAsset | 0 / 101 / 0 |
| public.subscribeToOrderBook: omit child subscribe | native dispatch trace:public.subscribeToOrderBook | 0 / 101 / 0 |
| public.#handleLastTradePrice: increment parsed price | native Number bits | 0 / 101 / 0 |

[Control summary](evidence/ws-dispatch/controls/summary.json) and17 raw logs retain the shared nine-test baseline and each isolated broken/restored run. Every fault compiled and reached its named runtime assertion. Finally restoration returned exact original source bytes; the final source diff exited0. No survivor was observed in this eight-control floor. The driver exited0; its exact environment/command is recorded in control-driver.json.

Static sub-agent review identified a gap: replacing the handler registry during stats did not change the captured array's length, so the tests would not distinguish length capture before stats. No mutation survivor was measured for that earlier draft. The fixture was strengthened to append callback40 to the old array before registry replacement with99; native/source independently require callbacks11,13,40. Control3 exercises that exact ordering fault. Final static review verified the fix, boundaries, exact target mappings and function-local control anchors; readers stopped before mutation runs. Reviewer ran no Cargo/oracle/control gates.

Coverage floor: **8 bodies ×12 scenarios =96 source/native root calls**, plus **one** inherited-constructor handler failure probe, **97** calls per full passing nine-test corpus. The source capture runs once per process. A passing restored exact-row test executes12 native root calls, while its single source capture still runs97 source root calls. Registered connection error callbacks replay once when actually installed; returned book cleanups replay twice when actually returned, with independent per-scenario replay expectations. These replay calls are separate from the97 body calls. Per-body enumeration stops at scenario11; full base corpus stops at fixture95, public #handleLastTradePrice/scenario11, followed by the single inherited-handler probe. Errorcallback floor stops at replay1; cleanup at replay2. Mutation floor stops at control8, public #handleLastTradePrice. Census stops at manifest7, polymarket.ws-public.ts.toml. These are finite floors, not complete venue proof.

No workspace compilation/tests, host clippy, wasm clippy, CI, MEXC gates, real network, wall-clock timers, general runtime implementations or full client-pipeline/wire gates were run.

## Final preservation

The last integration merge includes `85f3b9e596582fce3738807c4d9b7032c5b5a802`; all final scoped gates and controls were then rerun at `f454364be49323149a468d6745437653937b4100`. [Preservation receipt](evidence/ws-dispatch/merge-preservation.json) compares that gate SHA to evidence commit `b91785a626eac97ce3870e19f016c628ceee4cbc`: required venue src/tests diff0 and the extended29-input comparison0, including venue scripts/manifests, source venue/log severity/original JSON parser, seven dependency sources/manifests, Cargo.lock/workspace settings, target checker/extractor and package/lock inputs. Global extractor source/manifest/script inputs also diff0. Census remains58/18/65; source/test/script/manifest whitespace check0. Only evidence changed after gates. Prior exits remain attached to their measured SHA; no fresh scoped run at the evidence commit is claimed. The following receipt-only commit changes no gate inputs.
