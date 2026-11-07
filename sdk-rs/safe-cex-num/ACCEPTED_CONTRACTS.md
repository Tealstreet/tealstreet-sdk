# Num accepted-contract review

The dated core census selected 60 missing Num owners. Each contract in
`../safe-cex-parity/accepted-set/contracts/safe-cex-num.json` names its actual
compiler API and records its own body role or independently authored native
expectations. The eight existing master contracts are retained.

The selected native/default run produced 31 runtime owner results and 29
explicit classifications, with no failed owners. Classification receipts check
the exact API; the function-specific source/native tests named in those entries
provide behavioral evidence separately. The full Num suite, including ignored
source oracles, passed 93 tests under measured Node 24.20.0 before this batch;
the later stock-F64 and output-invariant supplements passed independently.

The selected run is the delivery boundary for this batch. The full package gate
currently stops before runtime at the separate source scanner's
`UNSUPPORTED_ATTRIBUTE_EXPANSION:macro_export` marker on the canonical
`js_number_output` definition. Its existing compiler/source/recipe verification
and macro API remain intact; the shared policy owner is handling that exact
definition's attribute seam. This batch does not claim a full-package pass.

The full source scan also requires an explicit root re-export list for the
finite `prototype_realm` module. Fresh rustdoc before and after the change
confirms all eight public names, target identities and full nominal definitions
are identical. The scanner's glob/attribute-origin guard remains enabled.
The nested `retained` conversion body is byte-identical to its previously
reviewed version; its whole-file source pin is reconciled after the separate
enum-dispatch repair.

The review keeps these roles distinct:

- Numeric, BigInt, date and input-buffer entrypoints are decoders with actual
  native results, numeric bits, variant identity or error payload observations.
- Serializer Capture, sequence and object builders have per-method native
  observations of retained nodes, builder fields and refusals. Capture covers
  all 27 compiler APIs. Unit-struct metadata now distinguishes the ModelField
  Undefined marker from ordinary Null through direct native observations. Only
  the empty tuple-struct builder uses an exact source-pinned constant policy.
  Object skip_field observations retain own Undefined keys for ModelRecord and
  omit them for ordinary structs; retained-node observers include Undefined.
- Prototype conversions and property APIs inspect actual variants, referent
  identity, realm state and mutations. Generic callback cases are bounded to
  the concrete witnesses stated in their entries.
- Borrowed property selectors return existing values or prototype markers.
  They do not acquire the classifications of the numeric coercion functions.
- Already typed arithmetic/policy operations, text transformations, property
  ordering and output writers are classified individually by their actual
  bodies. These are not crate-wide exemptions.
- `check_numbers` is a post-decode retention invariant. Its internal numeric
  node evaluation includes parsing `Node::Lexeme`; its role does not classify
  that parser or the actual ingress frontdoors away.

Executing these contracts exposed two production defects rather than narrowing
their input profiles: externally tagged enum maps failed before EnumAccess
dispatch, and stock Serde F64 events failed before retained JSON capture. Their
repairs and independent compiled controls live with `enum_numeric_ingress` and
the Types `retained_native_f64`/Num `passthrough_native_f64` targets.

`numeric_output_invariant` deliberately serializes a decoded negative number
with its sign removed. It requires the native retention guard to reject both
negative zero and negative finite values, at `$` and `$.payload[0]`, through raw,
owned and borrowed input paths. Positive controls still decode. The permanent
control inverts only the production bit comparison, requires the named runtime
assertion to fail, restores exact source bytes, and requires the target to pass.

Reproduce the package gate and the new control from `sdk-rs`:

```sh
node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-num --jobs=1
python3 safe-cex-num/scripts/numeric-output-invariant-control.py
```

Run the full Num suite with a measured Node profile accepted by `tests/gate.cjs`:

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/../node_modules/typescript" \
  cargo test -p safe-cex-num -- --include-ignored
```

These are bounded production native/default observations, not proofs for every
generic implementation or a feature/wasm coverage claim. The maintained
[numeric ingress contract](../parity/MANIFEST-SPEC.md#numeric-ingress-defect)
and [representation boundaries](../parity/MANIFEST-SPEC.md#divergence-taxonomy)
remain unchanged.

JsNumberInput's native observation covers the internal Undefined variant as well
as every pre-existing raw JSON case. Two additional fixtures capture ModelField
Undefined directly and inside an array through the real js_capture_output API,
then enter through the production from_retained conversion. The observed kind
keeps Undefined distinct from Null; the second array member independently pins
native 1.25 bits. This repairs stale generated observation code after the
presence cutover, without changing production decoder admission or APIs.

The same presence cutover names the newtype metadata argument `name` so the
ModelField Undefined marker dispatches `visit_unit`; ordinary named newtypes
still dispatch `visit_newtype_struct`. The accepted compiler signature now
records that reviewed argument name while preserving all entrypoint witnesses.
