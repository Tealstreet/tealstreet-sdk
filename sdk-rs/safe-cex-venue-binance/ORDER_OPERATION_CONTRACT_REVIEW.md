# Order operation function contracts

This review covers the five `order_operations` function owners assigned from the
Binance receipt census. All five are live in the constructed class path; none is
a test-only extraction. The public callable aliases retain their separate #938
contracts and native capability witnesses.

| Function | Role and evidence |
| --- | --- |
| `endpoint` | Closed venue and internal literal-key selector; a specific typed non-decoder classification. |
| `delete` | Capture, parameter evaluation, command invocation and await; a specific unit-returning command role, with no response/model decoder. |
| `snapshot` | Executed dense-array output contract with native bits/units, child fields, own undefined, retained object identity and exact guard errors. |
| `each_map` | Executed native map output, fresh result array, original object identity, initial length, live replacement, callback order and original failure identity. |
| `has` | Executed source property-presence cases for the update price key, own/inherited undefined and finite primitive guards. |

Native observations reuse `scripts/private-rest-observers.rs`; number bits,
UTF16 units, original object referents and presence are read before encoding the
observation. They do not project a production value into JSON before the helper.
These finite helper contracts do not widen the registry's separate typed
Order/UpdateOrderOpts admission, sparse arrays or executable/intrinsic mutation.
The configured parameter callbacks are capabilities, not deserializable models.

`tests/support/private_order_aliases.rs` drives actual native dependencies,
ExchangeOwner, factories, signing, Store and terminal HTTP. Its cancellation
witness holds the actual request, observes the exact textual ID beyond the safe
integer range, checks Open status before release and Canceled after settlement.
Its amendment witness checks cancellation before replacement formatting/POST and
the changed price with retained amount.

`scripts/order-operation-controls.py` disconnects each of the five production
helper bodies independently. Every fault must compile and fail its named real
native cancellation or amendment assertion; the exact original file must then
pass again. Compile failures and empty test selections are not successful
controls. This execution evidence is separate from the two classification
receipts and the direct output contracts.

Use the canonical accepted-set driver from `sdk-rs`:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" \
  node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-binance --jobs=1 --measure --module=order_operations
python3 safe-cex-venue-binance/scripts/order-operation-controls.py
```

Full compiler/dependency inventory and macro provenance stay enabled. Existing
alias contracts are retained. A measure exit zero is not sufficient: inspect
every scoped receipt for failures. This cohort does not close the remaining
Binance function groups, aliases or feature matrix.
