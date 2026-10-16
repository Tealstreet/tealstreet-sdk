# Polymarket socket lifecycle and subscription bodies

**8 pending → ported; 0 pending → divergent.** Polymarket triple: **42 / 18 / 81** (ported / divergent / pending). [Census](evidence/ws-lifecycle/census.json) compares all seven venue manifests against requested integration parent **`75c4e8562c72cf36338eaefaf5df8b9b4a4d0f2d`**. The independently specified eight-symbol set is checked exactly; census is distinct from runtime proof.

Fresh branch `feat/polymarket-ws-lifecycle-75c4e8` starts directly at that parent, without private unreviewed lane work as an ancestor. Before gates, integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` was merged down.

## Callable ports and limits

Public: onOpen, onClose, ping, #sendSubscription. Private: onOpen, onClose, ping, #authenticate. Each has an individual Rust function in src/ws_lifecycle.rs. Manifest notes explain every auxiliary SocketHost trait/method/associated-type and native frame/credential target. No exemptions were added. Full socket classes/constructors/composition and the concrete timer/transport host remain pending; these are callable method bodies at explicit receiver/runtime boundaries.

Open checks disposal once and preserves child-call order, including ping after missing private credentials returns normally. Close records synthetic diagnostics(undefined,true), logs, clears ping, assigns the ws property undefined, then checks disposal before scheduling reconnect. The reconnect callback has no extra disposal guard. Ping sends only when connected, then unconditionally registers 10000ms and replaces the property without cancelling older host-owned timers. Original host failures propagate with native identity and prevent later effects.

Subscription obtains one caller-provided insertion-ordered Set snapshot; empty snapshots return before sending/logging. Private authentication distinguishes absent credentials from present empty-string fields. Native UTF-16 units and well-formed JSON.stringify quoting retain lone surrogates, control characters and valid pairs. The supplied domains are ordered unique Set snapshots and ordinary credential records with required string fields; arbitrary JavaScript proxy/accessor side effects are not proved. Host callbacks resolve current receiver flags/child methods. Actual network execution, autonomous timer progression and full class wiring are not claimed.

## Gates attached to SHA

All following exits were measured at **`ddef2d2569bc5d42e8ead1cea9d79aee6b52bb69`**. [gates.json](evidence/ws-lifecycle/gates.json) contains exact commands; raw logs are tracked beside it. Source oracle environment uses `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Cargo commands ran sequentially.

| Gate | Measured result |
| --- | --- |
| Scoped cargo check, Polymarket, all targets, locked, jobs2 | 0 |
| Scoped Polymarket tests, locked, jobs2 | 0; 41 passed, zero failed; doc tests ran zero cases |
| Scoped cargo fmt check | 0 |
| Official venue-only invention/status checker | 0; 141 manifest rows, 88 public Rust items; zero venue exemptions |
| Actual Yarn parity:extract:check | 0; inspected output says would update 0 manifests, zero added/orphaned symbols |
| Eight isolated method-body controls | 0; shared nine-test baseline0, each mutation101 and restored exact-test0 |

A dirty draft fmt run failed1 and test compilation failed101 on syntax in the test harness. After correction an eight-test draft passed0, before adding the final full-native-before-wire and old-callback execution assertions. These development runs are not gates covering this SHA.

## Native proof and mutation floor

The oracle executes eight original TypeScript method bodies and original WS_CONFIG/WS_MESSAGE_TYPES/log-severity constants. It captures subscription/auth native fields before JSON.stringify. Native tests inspect field values, UTF-16 units, call order, independent counts, f64 delay bits, optional field presence, opaque timer identity/ownership, original error identity and reentrant disposal effects before any wire check. No roundtrip or native re-encoding comparison is used.

Eight row tests each run ten scenarios with two root calls: **80 cases / 160 native root calls**. The ninth wire test first repeats the entire native corpus, then compares **96 unique sends** against original TypeScript wire text. Thus the passing nine-test suite executes **320 native root calls**, while its single Node capture executes **160 source root calls**. The independent send expectation is 48 first-call sends × 2; accumulated trace entries are skipped, so prior sends are not recounted.

Registered callbacks replay all four disposed/connected combinations and a fifth live callback that throws the original failure. After a second successful ping, both runtimes explicitly invoke the first registered callback and require an independently expected one-call increment, proving that the old callback still executes after property replacement. This is captured-callback simulation, not a wall-clock timer test.

| Isolated production mutation | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| public.onOpen: omit sendSubscription | native socket trace:public.onOpen | 0 / 101 / 0 |
| public.onClose: omit clearPingInterval | native socket trace:public.onClose | 0 / 101 / 0 |
| public.ping: invert connected guard | native socket trace:public.ping | 0 / 101 / 0 |
| public.#sendSubscription: disable custom feature | native custom feature:public.sendSubscription | 0 / 101 / 0 |
| private.onOpen: omit authenticate | native socket trace:private.onOpen | 0 / 101 / 0 |
| private.onClose: omit clearPingInterval | native socket trace:private.onClose | 0 / 101 / 0 |
| private.ping: invert connected guard | native socket trace:private.ping | 0 / 101 / 0 |
| private.#authenticate: swap USER to MARKET | native subscription channel:private.authenticate | 0 / 101 / 0 |

[Control summary](evidence/ws-lifecycle/controls/summary.json) plus 17 raw logs retain baseline and individual broken/restored runs. Every fault compiled and reached the named runtime assertion; each function-local mutation restored original source in finally. No survivor was observed in this eight-control floor.

Enumerations stop at: fixture **79**, private.authenticate/scenario **9**, root iteration **1**; callback replay **5** (live original failure), with older ping callback execution checked separately; control **8**, private.authenticate; manifest **7**, polymarket.ws-public.ts.toml. These are coverage floors, not complete venue parity.

A static sub-agent review prompted the explicit older-callback execution witness and clearer ws-field-assignment naming. Both were fixed before these gates. Final review found no remaining actionable issue in this slice; reviewer ran no Cargo/oracle/control gates and stopped reading source before mutation runs.

## Not run

No workspace compilation/tests, host clippy, wasm clippy, CI, MEXC gates, real network or autonomous timer execution were run. Other 81 Polymarket pending rows remain pending.

## Final merge preservation

Latest pre-push merge includes integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a` at comparison head `15500a0e70e09b3ce9a5c44a24175e0ceee48038`. [Receipt](evidence/ws-lifecycle/merge-preservation.json) records gate-input diff exit0: venue src/tests/scripts/manifests/Cargo.toml, TS venue/log severity inputs, seven dependency sources/Cargo.toml, Cargo.lock, workspace settings, checker/extractor and package/lock inputs are unchanged. Foreign exemptions changed, but Polymarket exemptions remain independently expected0 and the scoped checker supplies its own empty exemption file. Prior scoped check/test/format/parity/control exits remain attached to `ddef2d2569bc5d42e8ead1cea9d79aee6b52bb69`; no fresh scoped run at the merge head is claimed. Fresh global extractor at `15500a0e70e09b3ce9a5c44a24175e0ceee48038` exited0 with zero manifest changes. The following evidence-only commit changes no gate inputs.
