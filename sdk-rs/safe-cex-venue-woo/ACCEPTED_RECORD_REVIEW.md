# Accepted declaration record batch

Thirty concrete record decoders are reviewed separately against their TypeScript
declarations: fifteen legacy owners from `woo.types.ts`, ten nongeneric owners
from `woo.v3.contracts.ts`, and five public owners from `woo.v3.public.ts`.
The earlier 32 string enum contracts remain unchanged in the Woo contract shard.

The input catalog was reviewed field by field, including wire renames and nested
records. Required `WooV3FuturesStats.id` and `symbol` remain required inputs even
though their native fields use `Option<String>` for a separate mapper behavior.
Optional properties have separate absent, present and explicit-null witnesses.
Legacy numeric order IDs retain their number/string variants; the seven V3
instrument unions likewise retain both native variants.

The 748 authored JSON-text cases execute four actual decoder paths: raw text,
owned Value, borrowed Value and qualified Deserialize. Native views read every
field and nested field, ordered vector element, enum discriminant and option
presence directly. Number observations compare IEEE-754 bits, including negative
zero and overflowing JSON number text (`1e400`). Decoded owners are not serialized
into their expected values. Each owner has a complete valid declaration-order
array beside valid objects, and required-field missing/null refusals. Unknown
extra properties retain the declarations' structural input domain. The corpus
uses ordinary UTF-8 strings and serde_json carriers.

This is declaration-decoder evidence. The separate unknown-input validation,
market/candle mapping functions, generic envelopes, number/string child owners,
and aliases retain their independent contract obligations.

Source discovery initially refused the protocol glob and five invocations of a
local validation macro in `v3_public`. The glob now names its eight actual imports.
The five numeric validation bodies are ordinary Rust, preserving evaluation
order, numeric conversion, finite/minimum checks and errors. The existing native
mapper test covers its 2,184 source cases before and after that expansion. The
local candle merge sorter stays in place with its existing non-decoder role;
its entire body and following code are byte-identical. Only its reviewed
whole-file source hash changes in the local-function policy.

The focused runner performs ordinary full-package compiler inventory before
selecting the thirty complete owners and checking their three source modules.
Reviewed non-decoder local helpers require their exact existing role and source
hash; unsupported events still fail. No alternate method, alias or private
helper is waived by these record contracts. Discovery retains 645 owners, ten
aliases and nine separately reviewed construction obligations.

Run from the repository root:

```sh
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-record-contracts.mjs
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-record-contracts.mjs --control
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-record-contracts.mjs --target=wasm32-unknown-unknown
```

The compiled control removes only each concrete record's object guard in copied
production code, after normal contract validation. Numeric capture and remote
schemas remain active. Every owner must fail its native output receipt when its
complete positional array is admitted; original production bytes are checked
after the run. Normal restored runs exercise the strict behavior again.

The mapper controls are maintained in scripts/accepted-public-source-controls.py.
Run it serially with no other Cargo process: it temporarily changes the candidate
market minimum and candle comparator, requires successful compilation followed
by the named native fixture failure, restores the exact bytes and requires a
passing test after each mutation. A finally block restores the source on an
error; receipts go into a temporary directory outside the repository.

Verified results: thirty production runtime receipts pass on native and on the
actual Wasm target, with zero failed owners. All thirty copied object-guard
removals compile and produce named native failures; exact native restoration
passes. Both maintained mapper mutations compile and fail the named fixture
test, with restored passes. Poisoning the local sorter's policy hash stops the
maintained runner at its named source-pin assertion before runtime probes.
Nine scoped tests (including the 2,184-case corpus and field-identity checks),
strict native/Wasm Clippy, formatting, and full parity also pass.


The later wave4 audit removed the unused WooxOrder, FetchOrderResponseMeta and
FetchOrderResponse DTOs and their contract rows. The selected retained cohort is
27 owners. Historical counts above describe the original review. The live
legacy-orders binding now covers the documented amount/client_order_id/
realized_pnl/average_executed_price nulls, which the erased declaration decoder
rejected. No runtime decoder was widened and no production request path changed.
