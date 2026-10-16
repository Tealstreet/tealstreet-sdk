# Crypto.com DTO finding repair

Integration parent: `0286f63437e4320e8c12886d5fef8fc6dfa2eac3`, merged before
final controls and delivery gates. This unit is local only; no push or self-merge.

All eleven original DTO explanation findings are **code-to-fix**, with **zero
genuinely rationale-only cases**. Their old unknown-property omission was a real
carrier loss. This repairs those carriers; generic REST/WS adapters still use
opaque values, so this is not a claim of eleven proven production-path bugs.
The earlier thirteen explanation findings therefore split **13 code-to-fix /
0 rationale-to-write**: two repaired in the preceding unit, eleven here.

| Source row | Classification | Repaired code surface | Named control |
| --- | --- | --- | --- |
| CryptocomInstrument | code-to-fix | unknown properties and current known-field projection | instrument-extra |
| CryptocomBook | code-to-fix | outer and nested BookData properties | book-extra; book-data-extra |
| CryptocomTicker | code-to-fix | unknown properties | ticker-extra |
| CryptocomTrade | code-to-fix | unknown properties | trade-extra |
| CryptocomCandlestick | code-to-fix | unknown properties | candlestick-extra |
| CryptocomPositionBalance | code-to-fix | unknown properties | position-balance-extra |
| CryptocomBalanceSummary | code-to-fix | outer and nested position properties | balance-summary-extra; summary-position-extra |
| CryptocomOrder | code-to-fix | unknown properties | order-extra |
| CryptocomAccountsResponse | code-to-fix | outer and nested identity properties | accounts-response-extra; account-identity-extra |
| CryptocomTradeFill | code-to-fix | unknown properties | trade-fill-extra |
| CryptocomError | code-to-fix | unknown properties | error-extra |

The governing scope is declared once in
[the DTO snapshot boundary](../parity/MANIFEST-SPEC.md#cryptocom-dto-json-snapshot-boundary).
Residual classification follows [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy) and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
Status credit remains **zero rows moved**, with **122 ported /15 divergent /
0 pending**. These eleven retain the measured FORCED string residual; fixing
unknown-field loss does not prove whole-row parity.

`tests/dto_retention.rs` independently names eleven source rows, stopping at index10
CryptocomError, and five nested record instances (two books, two balances, one
identity). Four decode entries give64 native record views. Sixteen unknown fields
per view exercise overflow, negative zero, underflow, index-like keys, duplicate
replacement, nested objects, own `__proto__` data and metadata-looking names.
A private-number-looking ordinary object is deliberately included. Native counts,
keys, typed fields and IEEE bits are asserted before source comparisons or wire
assertions within each test. Child wire expectations are independent literals and
source observations, never the encoded child itself.

The source witness executes the maintained complete TypeScript types module and
reads compiler-resolved schemas, then observes actual JSON.parse properties.
These eleven erased interface comparisons add **zero executable TypeScript
function rows**. They supply bounded DTO data-contract evidence, distinct from
the seven executable-reference rows counted in the earlier audit.

`tests/dto_controls.py` enumerates22 mutations: fourteen native extra-loss sites,
two numeric-admission guards, two native key/order cases, one stale typed-field
projection and three nested wire omissions. Each requires a compiled named101
assertion followed by exact source restoration and a named0 pass. Enumeration
stops at summary-child-wire-omission, iteration0 per call site. Its legacy book
wire-omission probe records any survivor explicitly; the new child assertion must
catch the same omission. This is a finite control floor, not exhaustive mutation
coverage. `tests/wire_controls.py` also keeps its full-object admission faults
working across the new capture boundary; no blanket36-control run is claimed.

Exact measured exits, control markers, survivors, source restoration and log
hashes are attached to the delivery SHA under local ignored
`sdk-rs/target/cryptocom-dto/<delivery-sha-prefix>/`. This document describes
witness scope; receipts determine which controls and gates actually completed.
The earlier e35f6ffc83 preflight exited101 in fixture setup, before DTO admission;
that is not runtime mutation proof. The corrected882e0ec641 preflight exited0
(11 new tests and3 legacy tests), and is historical, not a delivery gate.

The delivery bar is fmt and locked workspace build. Clippy, wasm, wiring, layers
and routing are centrally owned and are not run by this unit. Other Serde entry
combinations, full workspace tests, Linux execution, wasm execution and remote CI
are not claimed. No TypeScript, shared numeric-ingress, dependency or CI change is
part of this DTO repair.
