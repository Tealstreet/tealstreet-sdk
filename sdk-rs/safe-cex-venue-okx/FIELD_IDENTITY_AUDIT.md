# Field identity sweep

Baseline at 146c51139c: 35 ported / 16 divergent / 115 pending. Captured on
darwin arm64, Node v24.20.0. This is a new supplement, not a retroactive claim
that the original fixtures independently proved field identity.

## Mirror test scope

The archive-loader capture below is historical service evidence. Its replay
loader and five CSV archives live under `mobile-services/candle-server`, outside
this filtered SDK mirror. The removed `original_archive_loader_has_direct_column_identity`
test executed only that service loader; it exercised no SDK Rust implementation.
Its standalone archive oracle has also been removed. There is no presence skip
or replacement loader/fixture claiming to validate the absent production code.

The SDK field-identity tests remain: native decoded record/enum fields and the
maintained-source mapper supplement still run with `--include-ignored`. The
historical archive counts below are not current SDK gate results.

## Candidate census and limits

Executed all eleven original OKX oracle generators and recursively inspected
**24,155 emitted JSON objects / 3,493 equal-valued scalar sibling-pair occurrences**.
The checked-in candidate ledger lists each file, field pair, equal/distinct counts
and an example path. It covers inputs, output records, collaborator traces and
metadata; these counts are candidates, not 3,493 bugs. Runtime strings include
wire numeric strings and enums. Five null/null sibling-pair occurrences are recorded separately because their declared types cannot be inferred from emitted JSON. Serialization-erased NaN
cannot be recovered from this census; the separate NaN audit executes bit capture.

Read all five original archive files without changing their bytes:
**87,845 candle records / 279,453 equal numeric-column pair occurrences**.
The separate archive ledger records SHA256, exact pairs and contrasting rows.
Flat one-second candles account for many equal OHLC fields. There are also
contrasting rows, so this is not a claim that the whole archive is swap-blind.
The shared CSV loader remains a separate boundary from comparing two engines.

Neither number counts Rust fixture literals or runtime loop expansions as JSON
objects. Native fixtures were reviewed by their gate and construction path,
listed below; the JSON census is not presented as an exhaustive native-value count.

## Gate-by-gate test shape and disposition

| Owned family | Assertions and equal-value disposition |
| --- | --- |
| protocol tables and two literal unions | Authored aliases intentionally map to the same value (e.g. live/partially_filled). Swapping two identical authored table values is a semantic no-op, not corrupt field identity. Table comparison remains. New direct native region/instrument decode checks cover the two enum bindings, independently of their emitted wire names. |
| ticker mark/trigger policy and numeric ingress | Existing mark tests inspect native f64 bits over raw/owned/borrowed/nested/tagged paths. New distinct id/symbol/mark native assertions cover the same-typed string fields; a symmetric serde-name swap passes re-encoding but fails native id identity. |
| market policy | Fake Market id/symbol and nested precision/limit equalities are collaborators, not a claim that this venue implements the shared Market codec. The actual policy oracle checks selection, returned borrowed identity/order and symbol transformations. Shared Market decoder is owned by types. No shared-type swap claim is made here. |
| ledger/history/closed mappers | Original seeds include equal numeric prices/sizes/times. Added a separate distinct-field supplement for all three; native history and closed fields and native ledger PnL/fee/id are asserted before serialization. Later full-record comparison is additional, not the sole identity check. Original generators are unchanged. |
| journal adapter | Existing and supplemented results are in-memory JSON projections, not decoded PositionRecord or decode/re-encode identity. Added separate distinct prices, quantities, fees, PnL, times and IDs; full dictionary comparison detects projected field swaps. Shared PositionRecord parity remains unclaimed. Original 310 cases unchanged. |
| API endpoint policy | Fourteen cases are scalar input/boolean output, no two model fields. |
| REST operation witnesses | Batch payloads/response entries include repeated IDs and equal fake Order amounts. Assertions cover request bytes/order/count, source response errors, injected mapper arguments and returned store/callback records. The mapper recorder is a collaborator, not an implementation of mapOrders. No shared Order-codec claim. |
| response-phase witnesses | perMinute/total counters are naturally equal for the single request used. This gate asserts typeof and scalar bits, not counter-field independence or native factory parity; factory rows stay pending. |
| public dispatch/sequence | Equal trace counts/booleans are dispatch metadata. Per-handler identities, keys, source callbacks and sequence values are independently varied; handler order, first error and stats-before-callback have controls. No DTO decoder roundtrip claim. |
| ticker writers | Eleven input fields already varied independently against distinct unrelated inputs. Rust reads native DefaultStore fields and bits. Initial Store record fields all 1 are setup; per-field source patch cases independently verify writer identity. Existing volume swap controls discriminate currency/contract fields. |
| subscribe | Scalar/nested topic contents compare actual outgoing frames against source; aliases are declared separately. No decode/re-encode native model claim. |
| cache core/runtime/probes | Shared native candle helper OHLCV and filesystem/probe candles use distinct values. Persist/load equality alone is insufficient for symmetric codecs. Added seven native decoded chunk-record/Candle field assertions on raw/owned/borrowed paths and six direct KV consumer field assertions. Browser execution remains UNEXECUTED. |
| cache numeric ingress/owner Request | Existing numeric probes vary each field independently and read native fields/bits. Equal all-1 ingress seed has per-field perturbations; new distinct full-record supplement adds whole-field identity. Owner Request's four numeric field probes already read native fields, not roundtrip bytes. |
| cache calendar | Scalars compare direct results; split ranges assert each start/end field with contrasting inputs. Alias namespaces are literal protocol data, not DTO fields. |
| hydration null/error/timeout proofs | Candle values are setup for error/timeout/cancellation behavior, not their identity evidence. The separate cache decoder/KV supplements pin candle field identity; old proof bytes unchanged. |
| cancellation gate/waiter witnesses | Guard classifications, waker Weak release and task disposal are lifecycle properties; no decoded data-record/roundtrip parity is claimed. |
| service replay and service tests | Raw archives feed both engines through a shared loader, so byte parity alone does not certify loader column identity. Originals preserved; archive column census, actual-loader six-field supplement and cache native-field supplement are separate evidence. Existing HTTP cleanCandles sorting/dedup limitation remains. |

Four mapper families supplemented; four decoded type bindings (TickerMarkInput,
two unions, CandleCacheChunkRecord containing Candle) gain direct native assertions,
and one KV consumer boundary gains direct native assertions. No original shared
feeder was regenerated. No existing row becomes ported from this sweep.

## Outside-crate feeder inventory

The five service archives feed the service replay over both TS and Rust engines
and may be used by direct owner replay. Their exact bytes/SHA256 remain unchanged.
They contain equal-column rows, but contrasting rows exist; the identity supplement
is separate and does not relabel old replay greens. Cache native supplements run
alongside the old archive/proof artifacts, never replace them.

OKX oracle scripts are invoked only by OKX tests/root OKX gate, not shared numeric,
crypto or another venue corpus. Source import/reference inspection found no
external consumer of their emitted JSON. The scripts may import shared helpers;
that is the opposite direction from supplying their fixture output to a shared gate.
Original mapper/journal/market/REST fixtures are still preserved. The ticker NaN
capture is the sole in-place local repair, covered in NAN_CAPTURE_AUDIT.md.

## Controls

Each control must compile before a named assertion fails, then restore to passing:

- history amount/filled swap: `native history amount identity`;
- closed entry/exit price swap: `native closed entry price identity`;
- income fee/PnL swap: `native income pnl identity`;
- journal entry/exit price swap: `field identity supplement "journal"`;
- KV open/close post-decode swap: `KV consumer native field identity open`;
- symmetric TickerMark serde id/symbol name swap: re-encoded input remains equal,
  then `native ticker id identity` fails. This is the roundtrip-only blind spot,
  independently of unequal fixture values.

This file records the inventory/repair scope; passing a supplementary sentinel is
not a claim that every possible pair/CPU/input has been mutation-tested.

Confirmed original false green: swapping ClosedPnlEntry time and closed_time in compiling production code passed the original 1220-case mapper oracle at exit0 (ordinary closed uTime is zero, so both fields become open_time). The SAME mutation failed the new distinct-time supplement at101 on native closed open time identity. This is one confirmed corpus hole repaired by a separate supplement; old bytes and historical green meaning remain unchanged. The other five compiling swap controls prove supplemented field identities, not a claim they all survived the old gate. Symmetric serde-name corruption separately passed the explicit roundtrip check and failed native ticker id identity.

Historical archive supplement executed the ACTUAL replay-day load declaration over all 87845 original records and directly checks all six native JS fields against the CSV columns (527070 comparisons). A compiled TypeScript column-name swap fails at1 on archive loader field identity open; restore passes. This closes the shared-loader identity question separately from engine equality, without rewriting any archive. The compiling Rust KV consumer swap remains its separate downstream control.

## Precision selector audit

The maintained OKX exchange AST has ten market.precision reads in six methods:
updateOrder, updateTriggerAlgoOrder, mapOrders, mapPositions, formatAlgoOrder and
formatCreateOrder. Real manifest parsing finds every one still pending with no
Rust target. Implemented placement receives already formatted payloads; the
implemented mapper callbacks convert contracts rather than choose precision.
The ten production Rust source files currently contain no two-axis formatter
consumer to swap. This is zero implemented sites, not future swap immunity.
When those pending operations exist, their native payload corpus must distinguish
fractional price and amount steps, including trigger and bracket branches.

Pretty-symbol supplement: 655 maintained-source cases compare native optional presence, distinct base/quote strings and exact errors. Original market-policy corpus is unchanged; this new oracle feeds only this crate and runs one Node process for the whole corpus. The source swap exits 1; compiling Rust swaps exit 101 on `pretty-symbol native base quote identity 0`, and eager argument coercion exits 101 on `pretty-symbol cold branch must not coerce 390`. Valid JSON lone-surrogate acceptance and Rust UTF-8 refusal have separate escaped witnesses; this is a representation choice, not invalid JSON. No numeric bit capture is used by this string-output oracle.
