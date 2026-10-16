# Paradex divergence audit

Historical audit tree: `18878d8ca5556499c52b7dcb8e034e5dae2c62c6` (PR 7255).
The measurements below are historical; the current 26-row recheck follows at the end.

Real tomllib: pinned **9 ported / 24 divergent / 120 pending**. The 24 rows
comprise **23 CHOSEN / 1 FINDING**: 17 shared UTF-8 representation cases,
6 UTF-8 plus JSON-shaped-input restrictions, and one inherited crypto finding.
No row opens with FORCED, MIXED or CORRECTED. All 24 already opened with a
canonical bucket; this edit makes the per-row rationale explicit and names
ownership of the inherited finding rather than changing status to lower a ratio.

All **39 Rust target references** have public declarations in the pinned tree;
**0 rows have empty rs**. Therefore no row meets the requested no-Rust-target
condition for reverting to pending. Declaration existence is not itself evidence
of behavioral parity. The actual venue parity checker separately resolves its
mapped items; the two shared crypto targets were read and reviewed directly.

The manifest specification's shared UTF-8 anchor supplies a canonical reason
why local mirroring would be worse. The latest instruction additionally requests
an explicit sentence on every row, so all CHOSEN notes now carry that sentence
and retain the anchor. JSON-domain rows retain their additional local reason.
The follow-up getAccount row is also audited, outside the pinned 24: its typed
account-identity restriction is explicitly justified. Local totals remain
**10 ported / 25 divergent / 118 pending**; no new rows were implemented here.

## Every pinned divergent row

| Manifest | Symbol | Bucket / reason | Rust targets |
| --- | --- | --- | --- |
| `paradex.exchange.ts.toml` | `normalizePositiveFraction` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.exchange.ts.toml` | `ParadexExchange#convertIntervalToTradingViewResolution` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.income.ts.toml` | `toNumber` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.income.ts.toml` | `mapParadexFillToIncomeRecords` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.order-history.ts.toml` | `toOptionalNumber` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.order-history.ts.toml` | `toNumber` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.order-history.ts.toml` | `mapParadexHistoricalOrder` | CHOSEN: UTF-8 + JSON input boundary | 1 |
| `paradex.sdk.ts.toml` | `signParadexTypedData` | FINDING: FINDING / primitive coverage only | 2 |
| `paradex.types.ts.toml` | `isParadexTriggerOrderType` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `isParadexMarketOrderType` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `ACCOUNT_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `BALANCE_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `MARKET_RESPONSE` | CHOSEN: UTF-8 boundary | 3 |
| `paradex.types.ts.toml` | `ORDER_RESPONSE` | CHOSEN: UTF-8 boundary | 7 |
| `paradex.types.ts.toml` | `BATCH_ORDER_ERROR` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `BATCH_ORDER_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `BATCH_CANCEL_ORDER_RESULT` | CHOSEN: UTF-8 boundary | 2 |
| `paradex.types.ts.toml` | `BATCH_CANCEL_ORDER_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `POSITION_RESPONSE` | CHOSEN: UTF-8 boundary | 3 |
| `paradex.types.ts.toml` | `TICKER_RESPONSE` | CHOSEN: UTF-8 boundary | 2 |
| `paradex.types.ts.toml` | `ORDERBOOK_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |
| `paradex.types.ts.toml` | `TRADE_RESPONSE` | CHOSEN: UTF-8 boundary | 2 |
| `paradex.types.ts.toml` | `MARGIN_CONFIG_ENTRY` | CHOSEN: UTF-8 boundary | 2 |
| `paradex.types.ts.toml` | `MARGIN_RESPONSE` | CHOSEN: UTF-8 boundary | 1 |

## Witness and unresolved-work boundaries

- The 16 types-file rows use actual-module predicates or compiler-checked identity
  witnesses over actual declarations, native decoded fields, numeric bits and
  surrogate refusal. They do not claim validation parity with unchecked Axios.
- The interval helper executes its actual method body; its remaining restriction
  is the shared string boundary. Its exchange lifecycle stays pending.
- The six JSON-domain helper/mapper rows execute actual TypeScript bodies;
  comparisons observe native numbers/fields and coercion errors before JSON
  projection. Their callback/prototype/non-JSON restrictions are chosen, not
  described as impossible in Rust.
- `signParadexTypedData` remains FINDING. The existing `hash_typed_data` and
  `sign` declarations are shared primitives, not a completed signing adapter.
  Owner: Paradex venue owner for composition and an actual-venue witness;
  safe-cex-crypto owner for the disclosed `+1` / `0X1` felt coercion wrong hashes.
  Existing crypto evidence lives in `safe-cex-crypto/GATE_EVIDENCE.md` and its
  Starknet SDK corpora. No crypto tests were rerun and no shared fixture changed.
  Venue call sites are `ParadexRequestSigner.signOrder`,
  `ParadexRequestSigner.signModifyOrder`, and
  `ParadexJWTManager.generateNewToken`. Remote signer routing,
  `account.signMessage`, felt-pair formatting and returned headers still require
  the venue witness. Target existence does not close any of these gaps.

## Audit method

Parse pinned and local manifests with Python tomllib, inspect the source and
witness boundaries for every row, resolve public target declarations in a single
batched `git archive` capture, and run the real scoped parity checker. The first
scratch audit used 47 Git subprocesses (8 manifests + 39 target references); it
was replaced by one Git subprocess for the whole corpus. No Node-per-row loop
or Cargo suite was introduced for this note-only audit.

The precision audit is separately inventoried **per output field**, not inferred
from the venue-wide negative count; see `PRECISION_SWAP_AUDIT.md`.

## Scoped verification

The actual extractor compared eight venue files and 153 identities with zero
drift. Venue-only parity passed **88 rows / 48 public Rust items**; the direct
layer audit validated two edges. Parsed global exemptions: **359 blocks / 359
distinct**, zero Paradex blocks. `git diff --check` passed. This is a manifest
note/documentation audit, so no runtime or mutation reruns are claimed.
Sub-agent review independently confirmed the 24-row bucket counts, all 39 target
references, zero missing targets and the per-field precision inventory.

## Recheck after fetchBalance (47bb893dee)

Real tomllib over all eight files: **12 ported / 26 divergent / 115 pending**.
All 26 divergent notes already meet the requested opening-bucket and reason
shape: **25 CHOSEN / 1 FINDING**. Every CHOSEN note includes the canonical
shared UTF-8 citation and an explicit why-mirroring-would-be-worse sentence.
The FINDING retains its owner, wrong-hash reason, actual venue call sites and
missing composition witness. No status or note was changed to improve a ratio.

There are **42 divergent Rust target references**, zero missing/empty targets.
This is a declaration/note census, not coverage: the inherited crypto targets
remain primitive-only evidence and do not close signParadexTypedData.
`python3.14 scripts/check_divergence_notes.py` reproduces this census from the
current worktree with stdlib tomllib and **zero subprocesses**. It also refuses
ported rows with empty rs. The real scoped parity checker remains separate.

| Manifest | Symbol | Bucket | Target references |
| --- | --- | --- | --- |
| `paradex.exchange.ts.toml` | `normalizePositiveFraction` | CHOSEN | 1 |
| `paradex.exchange.ts.toml` | `ParadexExchange.getAccount` | CHOSEN | 1 |
| `paradex.exchange.ts.toml` | `ParadexExchange.fetchBalance` | CHOSEN | 2 |
| `paradex.exchange.ts.toml` | `ParadexExchange#convertIntervalToTradingViewResolution` | CHOSEN | 1 |
| `paradex.income.ts.toml` | `toNumber` | CHOSEN | 1 |
| `paradex.income.ts.toml` | `mapParadexFillToIncomeRecords` | CHOSEN | 1 |
| `paradex.order-history.ts.toml` | `toOptionalNumber` | CHOSEN | 1 |
| `paradex.order-history.ts.toml` | `toNumber` | CHOSEN | 1 |
| `paradex.order-history.ts.toml` | `mapParadexHistoricalOrder` | CHOSEN | 1 |
| `paradex.sdk.ts.toml` | `signParadexTypedData` | FINDING | 2 |
| `paradex.types.ts.toml` | `isParadexTriggerOrderType` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `isParadexMarketOrderType` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `ACCOUNT_RESPONSE` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `BALANCE_RESPONSE` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `MARKET_RESPONSE` | CHOSEN | 3 |
| `paradex.types.ts.toml` | `ORDER_RESPONSE` | CHOSEN | 7 |
| `paradex.types.ts.toml` | `BATCH_ORDER_ERROR` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `BATCH_ORDER_RESPONSE` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `BATCH_CANCEL_ORDER_RESULT` | CHOSEN | 2 |
| `paradex.types.ts.toml` | `BATCH_CANCEL_ORDER_RESPONSE` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `POSITION_RESPONSE` | CHOSEN | 3 |
| `paradex.types.ts.toml` | `TICKER_RESPONSE` | CHOSEN | 2 |
| `paradex.types.ts.toml` | `ORDERBOOK_RESPONSE` | CHOSEN | 1 |
| `paradex.types.ts.toml` | `TRADE_RESPONSE` | CHOSEN | 2 |
| `paradex.types.ts.toml` | `MARGIN_CONFIG_ENTRY` | CHOSEN | 2 |
| `paradex.types.ts.toml` | `MARGIN_RESPONSE` | CHOSEN | 1 |
