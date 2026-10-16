# Coinbase: sixteen remaining FINDING rows

This is the historical sixteen-row selection. The later
[complete 33-row audit](DIVERGENCE_AUDIT.md) supersedes its current-work counts:
six further explanations were withdrawn. The frozen verifier below is for the
original manifest snapshot, not a current gate after those note changes.

Reviewed source pin: `c023025ac423238ec50e19f07d895994967eccc8`.
Integration `5fd278411cdce527ed977e8c98066dfbc8f5b9ae` was merged locally at
`d617a3d17e992fce46315249c6a532e4da808ebd`; a scoped
`git diff --exit-code` confirms that this crate and its manifests are unchanged
from the reviewed pin. The preceding native snapshot repair is already on this
branch; it is a separate deliverable, not a new closure by this classification.

Python `tomllib` measures **393 ported / 33 divergent / 0 pending**, 426 rows
in 16 manifest files. Selection uses divergent notes **beginning** with
`FINDING:`, yielding exactly 16 identities. A substring search also finds three
MIXED notes citing `NATIVE_FINDING_CLOSURES.md`; those are not open FINDING rows.
The explicit identities and manifest-note hashes are retained in
[ledger.json](evidence/finding-classification/ledger.json).

The split is **5 behaviour / 11 rationale or proof gaps**: **31.25% / 68.75%**.
All sixteen remain open. **Zero statuses, targets or manifest notes change.**
The enumeration is sorted manifest filename and source row order, index 0
`CoinbaseExchange` through index 15 `CoinbaseSpotExchange`, stopping at
`coinbasespot.exchange.ts.toml`. This unit does not audit the other 17 divergent
rows, all ported bodies, or arbitrary dynamic host implementations.

For this classification, a concrete missing implementation or wrong local
operation belongs to behaviour. An absent concrete binding/witness, without an
established wrong local operation, belongs to the rationale/proof group. The
latter group is unresolved evidence, not eleven supported explanations or
eleven claims of correct behaviour. A dependency's defect is recorded separately
from the local imported-capability forwarding body, rather than counted again
on every possible caller. Thus the split is about the selected local rows, not
an estimate of end-to-end failing call paths.

| Index | Source row | Type | Evidence and remaining work |
| ---: | --- | --- | --- |
| 0 | `CoinbaseExchange` | behaviour | `src/classes.rs` retains construction, live portfolio slots and receiver builders, but `lifecycle_receiver` still takes an externally supplied host. Startup, tick, local methods and API/socket parent dispatch lack a complete concrete class join. The fourteen earlier policy scenarios prove a partial repair, not those missing implementations. |
| 1 | `CoinbaseExchange.start` | rationale/proof | `exchange_lifecycle::start` handles the synchronous outer update result, awaits its Promise, then connects. `LifecycleHost.update` admits synchronous native errors; a real Store publication/failure witness is absent. No new wrong local await was established. Its private-start/tick dependencies retain their separate findings. |
| 2 | `CoinbaseExchange._startPrivate` | rationale/proof | Calls `LifecycleHost.tick` and awaits it. The missing concrete tick binding is not a measured failure of this local forwarding body. Tick's separate message defect remains open. |
| 3 | `CoinbaseExchange._startPublicDataOnly` | rationale/proof | `drop(host.update(...)?)` propagates the synchronous outer failure while discarding the returned Promise. The type can express the source synchronous Store throw; the concrete commit/fanout and failure witness is missing. Discarding the Promise alone is not a demonstrated defect. |
| 4 | `CoinbaseExchange.tick` | behaviour | `exchange_tick.rs:155` reads only `e.value.message`. `PortfolioThrown::from(StoreMutationError)` retains the native error but sets `value=None` at `portfolios_balances.rs:46`. Returning that legal native failure from `TickHost.update` therefore forwards the error with message `None`, whereas source `tick` passes `err?.message` to `emitAndLogError`. Retained origin alone does not repair property access. |
| 5 | `CoinbaseExchange#schedulePrivateUserStreamWriteCheck` | rationale/proof | The timer callback returns `mark_private_user_stream_degraded`'s `Result` directly. The real health-helper Store failure has not been bound and witnessed through this timer. No local swallow was established. |
| 6 | `CoinbaseExchange#reconcileOpenOrdersFromRest` | rationale/proof | `update(...)?` handles the synchronous outer error; the catch passes the same `PortfolioFailure` to `emit_orders_fetch_error`. A concrete Store commit, stopped fanout and unchanged native-cause witness remain missing. |
| 7 | `CoinbaseExchange#scheduleOpenOrdersRestReconciliation` | rationale/proof | Timer starts the mapped hot reconciliation Promise. The concrete notification-binding evidence is missing in the callee; that is not proof of a distinct timer-body mismatch. |
| 8 | `CoinbaseExchange.cancelOrders` | rationale/proof | `CoinbaseOrderFailure` is JSON-valued, but source consumes the error and logs selected properties. Original native cause retention in the return is not required. Injected-error tests do not establish real committed Store state, stopped subscribers or cancellation/timer suppression. A deliberate property projection could implement this contract; its concrete binding is unproved. |
| 9 | `CoinbaseExchange.updateOrder` | behaviour | The native edit catch at `order_edit.rs:387` reads only `e.value`. If a permitted native failure escapes `schedule_reconciliation` or `schedule_write_check`, it falls through to `js_string(None)` and logs `"undefined"`; source uses the original error's message before falling back to `String(err)`. The native error itself is returned unchanged, so this is a message-policy defect, not a cause-retention defect. |
| 10 | `CoinbaseExchange.placeOrder` | rationale/proof | Source logs and replaces the thrown error. Its JSON-valued carrier need not return the original native cause. Real Store commit, subscriber suppression and timer-stop evidence is missing; existing injected JSON errors prove only the selected-message/replacement policy. |
| 11 | `CoinbasePrivateWebsocket.onMessage` | rationale/proof | The concrete class snapshot branch now retains the native failure and stops later events. Health, auth-failure and live-order capabilities still lack concrete Store failure witnesses. Arbitrary supplied handlers and the raw-order callee's separate defect are not counted as newly demonstrated local parser defects. |
| 12 | `CoinbasePrivateWebsocket#handlePrivateStreamError` | rationale/proof | Auth failure directly returns the imported health callback's `Result`; no local error conversion is performed. The typed health-helper bridge and its failure observation remain unproved. |
| 13 | `CoinbasePrivateWebsocket.handleUserEvent` | rationale/proof | Health and order callbacks propagate through `?`, stopping subsequent work on an error. Their concrete native bindings remain unproved; the retained snapshot failure is already witnessed. The raw-order callee's separate boundary defect remains open. |
| 14 | `CoinbasePrivateWebsocket.handleUserEventOrders` | behaviour | Source copies raw `order_id` and `client_order_id`. `order_sync.rs:341` converts absence to the literal string `"undefined"`; line 421 then stores that as an owned parent ID. Missing source IDs and literal `"undefined"` keys therefore cannot stay distinct. Raw product IDs are also coerced before lookup and UTF-16 text is narrowed. The existing nine-case source-only floor does not establish paired native boundary parity; this row additionally retains the unwitnessed live Store/fill-suppression binding. |
| 15 | `CoinbaseSpotExchange` | behaviour | `classes.rs:565` wraps the incomplete base class and replaces only the portfolio fields before dereferencing to it. The corrected override routing does not install the missing startup/tick/Store/local-method runtime. This shares the base-class cause but is a separate source row. |

The five behaviour rows cover three mechanisms: incomplete class composition
(two rows), native error-property access (two rows), and raw order-event
representation (one row). They are static source/native findings in this unit,
not five newly executed runtime failures. No new production mutation control or
Rust/source oracle was run for this documentation-only classification. Earlier
snapshot controls and venue tests retain their own measured revisions in
[GATE_EVIDENCE.md](GATE_EVIDENCE.md); they do not prove these open cases.

The independently reviewed split also examined native failures at the relevant
notification boundaries in these sixteen rows. For cancel/place, the source deliberately
consumes/replaces the exception, so an inability to return its original cause
is not substituted for a defect. For the public-only update, the synchronous
outer `Result` is distinguished from rejection of a discarded Promise. These
are precisely why not every notification-reachable root is classified as a
proved behaviour defect.

Canonical references remain MANIFEST-SPEC
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Run `python3.14 evidence/finding-classification/run-gates.py` from this crate
after committing. Final fmt/build exits are recorded by exact commit in the
local `.aimux/coinbase-finding-handoff/<SHA>/gates.json` receipt at the worktree
root. The inventory-only same-length identity and count-preserving category-swap
controls each failed at their named assertion with exit 1 and passed after byte
restoration with exit 0. Their receipts do not claim production behaviour
mutations. Review caught the earlier totals-only classification guard before
handoff; the final guard pins the five behaviour identities independently.

Host/wasm clippy, wiring, layers and routing are centrally owned and
were not run by this unit. A cancelled integration parity job remains an
unconfirmed CI result.
