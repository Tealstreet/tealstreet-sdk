# Exchange assembly bridge contracts

This first exchange_classes cohort covers exactly failure, json_value and typed.
The canonical accepted-set driver retains the full compiler inventory and measures
only these three owners. None is a classification-only receipt.

failure takes the existing String boundary and constructs the actual shared Error
inside Rc. Six messages observe name, message UTF16 and absent code/original
error/response. This does not claim arbitrary native JS Error construction.

json_value is the existing native-data-to-JSON carrier bridge. Nine profiles
observe returned object iteration order, nested lengths, string UTF16 and number
bits before serializing observations. Undefined rejects; the explicit nonfinite
profile records JSON's null replacement, omitted object undefined and zero
normalization. This documents a bounded carrier, not unrestricted graph identity.

typed is instantiated only as Vec<Market>. Eight profiles include two distinct
rows, empty output, malformed containers/rows, and native NaN, infinity and
negative zero. Every returned Market field is read before observation JSON:
required strings/active, row order, precision/limit/fee/contract number bits,
optional flags and absence of option/info in this corpus. This does not classify
other generic instantiations or opaque info graphs. Malformed-row observations
check Error kind and nonempty message, without claiming an exact diagnostic.

The witnesses include production functions through maintained compiler probe
instrumentation, never copies of their bodies. scripts/exchange-bridge-controls.py
runs three independent compiled semantic faults, requires the corresponding named
runtime receipt to fail while siblings pass, then restores exact production bytes
and requires all three receipts to pass. Execution receipts belong in PR validation.

Run the maintained measurement from sdk-rs (the separate target directory keeps
rustdoc/probe artifacts apart from ordinary native tests):

```sh
RUSTC_BOOTSTRAP=1 CARGO_BUILD_JOBS=1 \
CARGO_TARGET_DIR="$PWD/target/accepted-inventory" \
node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-binance --jobs=1 --measure \
  --owner=exchange_classes::failure \
  --owner=exchange_classes::json_value \
  --owner=exchange_classes::typed
python3 safe-cex-venue-binance/scripts/exchange-bridge-controls.py
```

The controls also accept `--only=failure,json_value,typed` subsets for replay;
every measurement still runs all three owners and requires sibling receipts to
stay green. Compiler errors never satisfy a mutation control.

Executed validation: canonical missing-classification RED for all three owners;
GREEN3/23 with no classification-only receipts. Each diagnostic, JSON-carrier
and typed-result body fault compiled, failed exactly its named owner with both
siblings green, and passed all three after exact source restoration. The final
source file was byte-identical to the committed production file. Failed compiler
attempts were rejected by the control runner and are not part of these receipts.
