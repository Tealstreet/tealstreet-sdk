# Hyperliquid order inference: SHA-bound receipt

**Moved 3 rows pending→ported, 2 pending→divergent: 5 total.**
Independent tomllib parsing compares every Hyperliquid manifest row to integration
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`:531 rows in both trees, no added or
lost keys. Baseline63/104/364 becomes66/106/359. Model/source enum and strict TIF
are ported; numeric/inference functions are CHOSEN for the independently
witnessed shared UTF-8 ingress boundary. All 13 original findings remain named
individually; no older row was promoted by this slice. The sweep stays OPEN.

Measured code SHA: `d09515e03032982f33fc362e472c5d7a91ff0c1c`.
Actual direct parent built on: `19e4e0ccb726ce2d4f3ef1229bcb8b7331312737`.
The branch fast-forwarded to the integration baseline before the survivor
classification and port. Sub-agent review covered the final source, tests,
manifests, survivor pair and all local ancestors; no review blocker remains.
No human review or workspace readiness is claimed.

History correction: a9590c4100 accidentally captured a temporary literal test
mutation when a control began before a pending commit/hook finished. Ordinary
commit19e4e0ccb7 restores the guard, preserving history. The earlier control
run is not used as SHA-bound evidence. Every gate below was freshly measured
at d09515e030, after that repair; after all controls, production and test files
match the committed tree. The user-provided .aimux-priority.md is kept untracked.
The later evidence-only commit is not claimed as newly tested.

| Gate / scope | Measured exit | Result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid --all-targets --jobs 2` | 0 | Scoped host compile floor |
| Exact Yarn1.22.21 `parity:extract:check` entrypoint | 0 | Output inspected: 0 manifest updates / 0 added / 0 orphaned;7341 symbols across512 source files |
| Five individual production mutations, exact order_inference Cargo test | 101 each | Named native assertions below; each compilation reached `running 1 test` and `test result: FAILED.` |
| Restored exact order_inference Cargo test | 0 | 1 passed / 0 ignored; actual module corpus62 cases:34 inference,16 numeric,12 TIF; three source-only escaped-surrogate witnesses, all eight source labels |
| Actual strict TypeScript literal pair program | 0 | Empty string/null both rejected2322; unifiedAccount accepted; diagnostic messages differ |
| Literal pair `--control-widened` | 1 | Named `LITERAL-SOURCE rejected pair and distinguishing positive` assertion; source oracle discriminates |
| Original literal content-bypass control | 0 | Retained FINDING: same rejection boolean; exact case/value pair below |
| Distinguishing accepted-input literal content-bypass control | 101 | `LITERAL-NATIVE rejected result HyperliquidUserAbstractionMode` |
| Restored exact literal accepted-set Cargo test | 0 | 1 passed / 0 ignored |
| Exact `surviving_rejection_pair_matches_actual_source_assignability` Cargo test | 0 | 1 passed / 0 ignored; native UnifiedAccount enum variant asserted directly |
| `python3.12 scripts/audit-row-dispositions.py --check ROW_DISPOSITIONS.json` | 0 | 66/106/359; all earlier63 roots and original findings preserved |
| Independent TOML movement census | 0 | Exactly3 pending→ported and2 pending→divergent, names/targets below |

All Cargo commands used `--jobs 2`, sequentially, with no concurrent Cargo.
Native tests/oracles used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The exact new-row test command was
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid --test order_inference actual_source_inference_preserves_native_fields_and_precedence --jobs 2 -- --exact`.
Literal controls used the committed runner with
`--target literal --only replace-content-bypassed replace-with-accepted-content-bypassed`;
that runner adds `--locked --offline`. No old full control matrix was rerun.
Native result fields, enum variants, price/helper bits and TypeError name/message
are inspected directly, with no production record serialization/roundtrip.

## Five new-row controls and restoration

| Mapped row | Mutation | Named observation |
| --- | --- | --- |
| toNumberOrZero | Add1 to the parsed finite number | Native caller price for Limit/91.25 becomes92.25; fails `INFERENCE native price bits` before later helper cases |
| HyperliquidOrderInferenceResult | Add1 to the result price member | Empty order native market price0 becomes1; fails `INFERENCE native price bits` |
| HyperliquidOrderInferenceSource | Replace TriggerPxOnly output with OrderTypeTp | Trigger-only41.5 case fails `INFERENCE native source label` |
| inferHyperliquidOrderTypeAndPrice | Match lowercase limit instead of Limit | Concrete Limit+trigger TP input (limit91.25,trigger17.5) becomes TakeProfit instead of Limit; fails `INFERENCE native order type` |
| parseHyperliquidOrderTimeInForce | Map Alo to GoodTillCancel instead of PostOnly | Native Alo result fails `INFERENCE native tif` |

```text
SHA d09515e03032982f33fc362e472c5d7a91ff0c1c
numeric-helper exit 101 INFERENCE native price bits
native-result-price exit 101 INFERENCE native price bits
native-source-variant exit 101 INFERENCE native source label
limit-precedence exit 101 INFERENCE native order type
strict-tif exit 101 INFERENCE native tif
restored inference exit 0; 1 test passed
```

These are one focused discriminating mutation per new row, not a complete
input/output cardinality sweep. Numeric-helper mutation sensitivity is measured
at its inference consumer before the direct helper loop; no helper-loop control
is invented. Source cases include truthy string zero, false/array/object fields,
unknown nonempty orderType labels, TP-before-SL and poisoned toString ordering.
Source/Native UTF-8 divergence is explicit for escaped orderType, limitPx and
numeric-helper strings; getter/proxy host objects remain outside the JSON adapter.

## Survivor classified, retained and inventoried

FINDING call site:
`tests/literal_unions.rs::actual_serde_literal_sets_match_ts_and_reject_widening`.
Concrete original mutation: HyperliquidUserAbstractionMode rejected input `""`
replaced by `null`, with only the input content guard bypassed. The actual
strict compiler's declared-type assignability boolean is false for both;
it genuinely supplies no distinction on that observable. Their values and
source diagnostic text differ, and neither is asserted equivalent. The original
control remains expected exit0. The additional `""`→`"unifiedAccount"` control
crosses the accepted-set boundary and fails at the named native rejection
assertion with the same guard bypass. Source widening to any also fails by name.
The call site/pair is inventoried in CARDINALITY_AUDIT.md and the control runner;
this control finding is separate from the 17 parity FINDING rows.

```text
literal/replace-content-bypassed: exit 0: test result: ok. 1 passed
FINDING: HyperliquidUserAbstractionMode empty-string -> null survives at literal_unions.rs::actual_serde_literal_sets_match_ts_and_reject_widening with content guard bypassed; actual strict TypeScript checker rejects both (2322). Acceptance is equivalent; values and diagnostic messages differ. See literal-rejection-equivalence.cjs.
literal/replace-with-accepted-content-bypassed: exit 101: LITERAL-NATIVE rejected result HyperliquidUserAbstractionMode
literal/restored: exit 0: test result: ok. 1 passed
```

Actual-source pair output (before any native wire comparison):

```json
{"type":"HyperliquidUserAbstractionMode","observable":"strict declared-type assignability only; values and diagnostic messages differ","cases":[{"raw":"\"\"","accepted":false,"codes":[2322],"messages":["Type '\"\"' is not assignable to type 'HyperliquidUserAbstractionMode'."]},{"raw":"null","accepted":false,"codes":[2322],"messages":["Type 'null' is not assignable to type 'HyperliquidUserAbstractionMode'."]},{"raw":"\"unifiedAccount\"","accepted":true,"codes":[],"messages":[]}]}
```

## Independent row census

```json
{
  "sha": "d09515e03032982f33fc362e472c5d7a91ff0c1c",
  "baseline_sha": "2bc83d78d4f821c4b116d082d02c7b5a2595ffc5",
  "rows_moved": 5,
  "pending_to_ported": 3,
  "pending_to_divergent": 2,
  "before": {
    "pending": 364,
    "ported": 63,
    "divergent": 104
  },
  "after": {
    "pending": 359,
    "ported": 66,
    "divergent": 106
  },
  "rows": [
    {
      "manifest": "hyperliquid.order-inference.ts.toml",
      "symbol": "HyperliquidOrderInferenceSource",
      "from": "pending",
      "to": "ported",
      "target": "safe-cex-venue-hyperliquid/src/order_inference.rs::HyperliquidOrderInferenceSource"
    },
    {
      "manifest": "hyperliquid.order-inference.ts.toml",
      "symbol": "HyperliquidOrderInferenceResult",
      "from": "pending",
      "to": "ported",
      "target": "safe-cex-venue-hyperliquid/src/order_inference.rs::HyperliquidOrderInferenceResult"
    },
    {
      "manifest": "hyperliquid.order-inference.ts.toml",
      "symbol": "toNumberOrZero",
      "from": "pending",
      "to": "divergent",
      "target": "safe-cex-venue-hyperliquid/src/order_inference.rs::to_number_or_zero"
    },
    {
      "manifest": "hyperliquid.order-inference.ts.toml",
      "symbol": "inferHyperliquidOrderTypeAndPrice",
      "from": "pending",
      "to": "divergent",
      "target": "safe-cex-venue-hyperliquid/src/order_inference.rs::infer_hyperliquid_order_type_and_price"
    },
    {
      "manifest": "hyperliquid.order-inference.ts.toml",
      "symbol": "parseHyperliquidOrderTimeInForce",
      "from": "pending",
      "to": "ported",
      "target": "safe-cex-venue-hyperliquid/src/order_inference.rs::parse_hyperliquid_order_time_in_force"
    }
  ]
}
```

Not run: whole venue suite, workspace compilation, host clippy, wasm clippy or
check, crypto tests, wiring/layer/parity runtime suites, old NULL/ZERO/STRINGS
mutation matrices, full monorepo suites, CI rerun or live venue calls. Old-row
four-class hardening is paused under the porting-first priority. Current
ROW_DISPOSITIONS.json retains their source-level evidence references and adds
the new inference callee inventory; it is not a per-row universal proof verdict.
