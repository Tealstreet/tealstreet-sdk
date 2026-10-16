# Shared info and configuration eligibility

The requested remaining graphs were five info-bearing models (`Market`, `Order`,
`Position`, `Ticker`, `Balance`) and three capability-containing graphs
(`ExchangeOptions`, `ExchangeOptionsExternalPublicData`, and its Variant1).
Variant0 was already registered. The census lists every registration; this change
also adds store_model's TaskOrigin, TwapStatus and SliceAction.

Registration alone was insufficient. Before extraction, the private Ticker info
companion reached Value::deserialize and all 15 probes refused: -0, -0.0,
1e400, -1e400, -1e-400, each through raw, owned and borrowed shared entries.
The wider five-model probe confirmed direct refusals, while tagged buffering
accepted finite values. Therefore acceptance alone is not the final instrument.

Types owns the canonical retained_json capture, mechanically extracted from num
in 61c0410b1a. The five private info companions call its present-field helper.
Num delegates capture and passthrough to it and retains the typed event adapter
and output guard. The node conversion is private; no unchecked public conversion
into JsNumberInput is added. There is no types-to-num dependency or second visitor.
The raw recursion limit, arbitrary-precision number grammar and NaN refusal remain.

Each domain model explicitly names `passthrough { info }`. Consumer rows must
name this field and retained capture, rather than treating all Value fields as
eligible. The public field stays Option<Value>: absent is None, explicit null is
Some(Value::Null). This does not change the numeric ingress adoption rules for
other typed fields: callers still need shared entry or deep Deserialize adoption.

The live Node corpus supplies 2,019 spellings. Five domain models across raw,
owned and borrowed entries, both direct and internally tagged, make 60,570 numeric
bit comparisons per runtime. Each model/path also checks absent and present-null
info. ExchangeOptions' nested tuning number adds 6,057 bit comparisons per runtime.
Its external websocket configuration remains eligible. All 24 tested capability
payloads refuse: the four marker enums remain uninhabited, and exhaustive empty
matches do not admit a runtime handle.

The stock-Value negative control deliberately projects native infinity to null
inside tagged Content. Its named assertion requires the output guard to refuse
at `$.value.info.nested[0]`. A production mutation bypassing that exact path
compiled and failed `shared info stock Value projection guard accepted a projection`;
the mutation was restored. The four pinned compile-fail fixtures continue to
refuse unmarked Value, references, skipped/renamed fields and nested containers.

Validation on the merged extraction tree: the 16-test numeric suite passed on
Node 24.20.0 and 26.7.0. All 29 num test binaries completed successfully (existing
ignored legacy tests retained); GRVT's scoped tests passed. Host strict Clippy and
wasm32 strict Clippy passed separately for num and GRVT. Formatting, extractor
zero drift, the layer audit, and CI wiring passed. Parity reported 7,334 rows and
2,781 public items. Merge validation retained all named CI steps (88/89 to 89)
and parsed exemptions as 338 blocks / 338 distinct targets, no blank reasons.

The incremental merge of integration 0968312f8d changes no num, types production
or GRVT source. Its wiring, layer and parity checks passed (7,334 rows / 3,210
public items); named steps 89/90 to 90, exemptions 338 blocks / 338 distinct,
with neither parent's targets missing. These counts describe that merge snapshot.

Integration advanced again to cd122af5d2 (KuCoin). The final incremental tree
retains 90/90 to 90 steps, 338 blocks / 338 distinct exemptions, and passes wiring,
layers and parity (7,334 rows / 3,211 public items). It has an inherited extractor
failure: one changed Coinbase starbase-orders manifest, zero added/orphaned,
explicitly recorded in cd122af5d2. Coinbase source and manifest are unchanged by
this merge. This is NOT an all-green final-tree claim; the earlier zero-drift
result describes the extraction baseline, not the newer consolidated tree.

The owner repaired that inherited drift at 18261b604c by adding seven canonical
blank separators. After merging it, real TOML parsing confirms identical data;
extractor now reports zero changed/added/orphaned, and parity remains 7,334 / 3,211.
No production, dependency, or CI files changed in this final formatting merge.

The final held integration head, 17f785479a, adds only worker code and two worker
manifests. Incremental review verified exact incoming-file preservation and no
changes to num/types/GRVT, CI, lock or exemptions. Parity and zero-drift extractor
checks passed again; the earlier production gates apply to identical source.
