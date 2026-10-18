# Bitunix venue port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/bitunix/CLAUDE.md).

The constants slice maps the 12 runtime declarations in `bitunix.types.ts`.
Tables retain source entry order and use shared enums. Consumers must use
`safe_cex_num::js_table_lookup` to retain stock inherited-member identity;
an own-only Rust map lookup would lose observable TypeScript behavior.
The witness loads the actual TypeScript module and actual shared enum module,
without enum stubs. It observes inherited typeof/String before serialization.
No fixture is shared or regenerated. Runtime mutation of exported tables is
outside this declaration slice.

Gates are scoped to `safe-cex-venue-bitunix`, with `--jobs 1`. Set
`SAFE_CEX_TYPESCRIPT_PATH` to the installed TypeScript package. Run package tests,
package fmt, strict host clippy and strict wasm32-unknown-unknown clippy separately.
`tests/controls.py` mutates one Rust declaration at a time and requires the named
constant assertion to fail, then restores the source and requires a passing run.
`tests/parity-check.py` runs the shared indexer with only this venue as a workspace
member and only Bitunix manifests; its scratch workspace stays under `target/`.
REST and websocket behavior remains pending; compose the shared exchange,
store, numeric ingress, crypto and transport contracts when adding it.

`wire.rs` maps the pending-position array declaration and KlineInterval union.
Positions capture through shared JsNumberInput before strict object decoding;
number output registration is explicit and exhaustive. Optional string fields
refuse present null. TypeScript annotations do not validate Axios responses, so
this slice claims declared shapes, not unchecked runtime input validation.
The wire oracle executes the actual TS module and resolves fields through the
compiler checker, including inherited fields. One Node process returns the full
corpus. Tests compare native fields and number bits on raw/owned/borrowed/direct
Serde entry paths, not just serialization roundtrips. During porting run only
`--test wire`; full scoped gates and mutations belong at slice readiness. Never
run concurrent Cargo commands. Use the real Yarn JS entrypoint if needed.

The response declaration records the shared UTF-8 boundary as CHOSEN:
`parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`. Oracle string observations
use UTF-16 code units so refused lone surrogates remain observable in the corpus.
All four literal enums implement string-only Deserialize; derived enum decoding
would also admit externally tagged objects, unlike the TS string unions.

Oracle diagnostics record Node, V8, CPU architecture and OS per corpus. NaN
observations alone canonicalize; signed zero remains bit-exact. Readiness control
scripts retain each failed/restored test log under `target/bitunix-controls/`
for SHA-specific sub-agent review; these generated logs are not committed.

The utility slice mirrors sha256Hex with workspace sha2 (the crypto crate has no
public single-SHA256 primitive). The witness executes the actual TS utility and
actual Noble dependencies in one process over 25 inputs. Scalar input parity
includes padding boundaries, NUL and non-normalized Unicode. The manifest marks
the shared UTF-8 boundary CHOSEN; four lone-surrogate inputs observe TS's
replacement digest without admitting lossy replacement at the Rust API boundary.
`tests/utils-controls.py` changes production hex output to uppercase, requires
`utility sha256Hex` to fail, then restores and passes the single utility target.

`order_ids.rs` maps the two virtual TP/SL suffix helpers. The witness parses the
actual class and executes its unchanged arrow initializers with their lexical
owner, without constructing a venue or importing network dependencies. One Node
process returns all 25 cases. Only the recognized final suffix is removed;
Unicode prefixes and repeated suffixes are retained. Both rows declare the
shared UTF-8 boundary, with source code-unit observations for refused surrogates.
Production controls independently corrupt leg recognition and prefix length.

`value_helpers.rs` maps three numeric/flag utilities with shared js_number and
js_trim. Its Option<&Value> domain is retained JSON plus undefined: callers must
capture through shared numeric passthrough before constructing Value. It does
not accept arbitrary JS host objects, native NaN inputs, symbols or BigInt; the
rows declare that CHOSEN restriction and the existing UTF-8 boundary. Number
outputs and explicit fallback arguments remain f64, including NaN and signed
zero. The actual-source oracle executes one 225-case corpus; number observations
canonicalize NaN only. Production controls independently invert each helper's
numeric decision and require its named assertion to fail, then restore and pass.

Before porting price/amount formatters, apply [the precision-axis audit](PRECISION_SWAP_AUDIT.md).
The current zero-site inventory does not establish future swap resistance.
Require fractional distinct-step source cases and direct native payload assertions
for limit, stop-market and bracket/amendment paths, with one compiling failing
precision-swap control per production site, then restoration.

Precision mutations are per field, not a venue-wide verdict: price, quantity,
trigger price, stop loss, take profit, bracket execution price and amendment
fields each need their own applicable swap result. Current census remains zero
implemented two-axis sites and zero fields swapped; four source formatter/
amendment consumers are pending. Never infer one field's coverage from another.

Journal grading belongs to packages/venue-data, which safe-cex must not import.
The measured Bitunix axes in journal/axis-grading.ts are positions and income;
available fills and orders do not inherit that measured identity. Preserve raw
axis rows and source provenance in the port; do not assign a venue-local grade,
filter collection on grading, or claim an unmeasured axis reconciled.

Cardinality is checked independently of pairwise values. See
[CARDINALITY_AUDIT.md](CARDINALITY_AUDIT.md) for two measured baseline false greens
and their repair. Preserve source total/per-case counts, native length equality,
and actual comparison totals. Empty AND partial production-output mutations must
compile and fail by name, then restore. The wire supplement runs with the original
oracle in one Node process; original oracle bytes remain unchanged.

The follow-up cardinality controls also bypass primary source counts and assert
independent executed comparison totals per helper and interval/export group.
Run `cardinality-six-controls.py` and `cardinality-seven-controls.py` sequentially.
Collection decoder controls must consume all input before returning None; a
non-consuming trailing-element error does not prove an assertion observes loss.
The consuming-empty control also removes both length checks and verifies the
remaining whole-vector equality fails by name. Keep census and mutation proof
separate, including which decode route reached the failing assertion.

`history_numbers.rs` mirrors the three numeric helpers in order-history and
income. These are parseFloat(String(value ?? '')), not Number(value): preserve
numeric prefixes, finite signed zero in toNumber, and zero-as-undefined in
order toOptionalNumber. Use shared js_string and parse_float; do not substitute
the exchange parseFiniteNumber helper. Inputs use the existing retained JSON
plus undefined and UTF-8 domain boundaries; arbitrary host objects remain out
of scope. One Node invocation executes all three actual arrow initializers over
51 inputs, with 49 native comparisons per helper and two source-observed UTF-8
refusals. The helpers remain separate source mappings even where bodies match.
`history-numbers-controls.py` proves one production mutation per row plus empty,
subset, extra and refusals-only corpus mutations with source guards bypassed.
History mappers and REST pagination remain pending; this utility slice makes no
journal grading claim and introduces no Store or precision-axis call site.

position_adapter constructs shared canonical PositionRecord from position history.
Keep qty (closed) separate from maxQty (peak), positive costs separate from rebates,
and explicitly opt in to funding subtraction when realized PnL is net of funding.
The positionMode field selects hedge leg and liqQty selects liquidation; unknown
directions refuse rather than default. Native settlement callbacks are infallible
and row input is retained JSON/UTF-8, excluding executable/getter inputs. Use the
focused --test position_adapter test, enabled in ordinary Cargo runs.
It needs Node and SAFE_CEX_TYPESCRIPT_PATH; no live account or network is used.
No fetch wiring is claimed.

**Response selection helpers:** `response_helpers` walks the seven source envelope edges in preorder through depth four without entering arrays. `row_extraction` selects the first qualifying array, preserves first-value precedence (including false/zero), and uses shared `TableHit` for inherited stock Object members. Inputs are retained JSON; account fallback is `None` for the source empty object.

**REST reads:** `rest_reads` supplies async market, ticker, balance and position-mode reads through a caller-owned GET closure returning `xhr.response.data`. Fallback and position callbacks read caller-owned store snapshots after the await boundary; lookup/report callbacks own host integration. The balance mode update happens before numeric parsing and survives a later fallback. Position-mode reads propagate failures. Shared `Market.limits.amount.max` is numeric even when the source passes through a string; this is declared in the manifest and source oracle. Report callbacks must perform the source trace and, for balance, emit/log the chosen transport/coercion cause.

**TP/SL helpers:** `tpsl_helpers` preserves nullish raw-ID precedence, position-disambiguated leg IDs, explicit raw trigger precedence and cached false reduce-only values. `TpslLeg` represents the inline TS tp/sl union and is mapped alongside the ID helper. Inputs use shared retained numeric JSON; arbitrary host objects and lone-surrogate strings remain declared boundaries. Recovery gates are recorded in tracked `GATE_EVIDENCE.md`.

**REST write orchestration:** `rest_writes` keeps store/transport/error hooks fallible so source catch boundaries remain observable. Mode changes inspect all fallible contract-number projections before posting, then set `isHedged`. Margin changes retain full UTF-16 symbol units, including lone surrogates. Symbol cancellation captures filter length and skips absent slots; batching hydrates against live length, then captures formatting length and flattens in order. Child cancellation/formatting/hydration implementations remain separately mapped rows. Leverage retains shared Math.min/Math.max NaN/signed-zero behavior and updates both long/short IDs only after the POST succeeds.

Concrete REST and private websocket signing read the shared `VIRTUAL_CLOCK`
(`get_current_time`, matching numeric Date coercion), while transport timers keep
the runtime clock. Both the sendLogin prefix and login-failure retry await
`sync_bybit_client_server_time(None)` in source order. The timestamp is captured
before the first sync, as in TypeScript. Native UUID consumers strip nanoid's
`-` and `_`; correlation request IDs retain the raw runtime ID.
`tests/native_clock.rs` exercises the concrete signed REST and websocket paths
with a different host wall clock and a pushed 40-second offset.
`tests/native_identity.rs` executes the actual source batch body and uuid utility
and compares generated IDs, null-row errors, retained prefixes and later chunks.
Optional response-envelope reads remain distinct from direct batch-row reads.
`tests/native_audit_controls.py` compiles each fault independently and retains
logs under `target/venue-audit-controls`.

Concrete market reads omit eligible rows with non-string base/symbol or a
refused maximum conversion and warn with the row, field and original value. Valid siblings remain fresh; all refused
rows produce an empty result. Quote filtering remains source-strict USDT.
Source/transport failures still log their cause before returning cached markets.
`tests/native_market_refusal.rs` exercises production fetch and refresh paths.

The native HTTP lane acquires its rate slot before each signing attempt, including retries and signature resends. `tests/rate_limit_signing.rs` fills the constructed venue’s five-request window and changes the synced clock while a private read is queued; its transmitted timestamp must come from dequeue time.

The production native query signer uses the shared JsRoot query profile rather
than pre-sorting UTF-8 bytes. Sorting applies recursively to objects and array
index keys, so repeat arrays with at least 11 entries follow localeCompare too.
The scoped `--lib native_query_tests` regression constructs the actual venue/API
client over the existing scripted wire and compares its signatures to the
maintained TypeScript createAPI interceptor with real qs and SHA primitives.
Its source oracle mode leaves the original factory/request/response corpus
unchanged. Primitive IO/clock fixtures do not replace the native client or signer.

The public placement preflight refuses nonfinite amount inputs before hydration
or HTTP, for both single orders and whole batches. It reports the amount and
original NaN/Infinity value through the existing error path and returns TypeError.
This is the explicit strict placement-input ruling; TS may format a NaN/-Infinity
quantity or produce no payload. It does not justify coercing an input to zero,
leaking a JSON decode error, or widening the placement carrier. The real native
identity test keeps the maintained-source observations alongside all6 refusals.


Attached virtual TP/SL amendments fall back only on the source unsupported-order
message. The reconciled payload is validated before cancel, including refusal of
LIMIT protection semantics. Cancel uses the limited xhr; only an accepted cancel
marks the exact parent and parent__children canceled in the real Store, before
calling existing placement. A failed replacement leaves that canceled status.
`native_identity` compares8 actual-source/public-binding paths, including both
legs, nonmatching/ordinary-parent errors, LIMIT preflight and cancel/place failures.
The existing retained snapshot/UTF-8 boundaries remain; placement inputs stay strict.



`native_identity::production_ticker_coercion_matches_constructed_typescript` exercises
80 field-independent cases through the constructed venue's actual socket and
DefaultStore. Null, numeric/prefix/NaN strings, objects, overflow, signed zero and
false arrive beside good rows as consecutive single-row protocol frames. The
maintained TS exchange and actual TS DefaultStore produce the comparison values;
eleven native number fields are checked before JSON. The shared ticker census
records the witness; `parity/scripts/ticker-native-controls.py bitunix` compiles
a production binding fault, requires the named assertion, restores and reruns.

Model-info reads and private Store patches use canonical RetainedJsonInput through
OrderModelHost and PrivateMessageHost. Typed model edges use from_typed/to_typed;
only a completed HTTP payload enters the existing request wire adapter. Native
info keeps number bits, own undefined, UTF-16 text and nested owned snapshots.
Raw identifier returns still use the shared String output boundary.

The retained native_identity controls drive real cancel/replacement requests for
nullish raw IDs, UTF-16 trigger/execution kinds and undefined sibling position IDs.
Private socket controls retain opaque metadata through position and order patches,
including a UTF-16 client-key cache miss. Malformed websocket JSON still maps to
null, matching the maintained jsonParse helper, and does not reconnect the socket.
Each repair has a compiling pre-fix failure and a restored passing native control.
