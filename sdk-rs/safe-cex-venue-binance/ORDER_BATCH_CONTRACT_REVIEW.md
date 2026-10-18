# Order batch function contracts

This cohort covers the five live `order_batches` function owners. The existing
`Post`, `Classifier` and `Log` aliases keep their separate capability contracts.

| Function | Individual role |
| --- | --- |
| `endpoint` | Generated static table lookup using a closed venue and internal literal keys; no response decoder. |
| `post` | Captures and awaits the configured POST, forwarding the native payload and result without interpreting either. |
| `dense` | Native dense-array output: cardinality, numeric bits, UTF16 leaves, own undefined, child identity and exact guards. |
| `iterable` | Executed configured array guard, including its native nonarray refusals. |
| `id` | Native optional response-ID conversion with nullish absence, exact large text IDs, paired Unicode, native numeric conversion and noncallable toString errors. |

The three output contracts contain 32 finite cases. Observations reuse the
canonical private REST observer and read the native values before encoding the
observations. The ID profile keeps the existing String boundary for lone UTF16;
it does not claim executable conversion hooks, mutable prototypes or sparse arrays.
The guard observations do not assert TS for-of equivalence for arbitrary built-in
iterables; the live typed entry supplies an options array. The two classification
receipts do not execute commands or classify caller-interpreted response models.

The constructed native witness
`private_order_aliases::native_bulk_placement_retains_post_result_until_signed_http_settles`
drives actual native dependencies, ExchangeOwner and Method::PlaceOrders. It holds
terminal signed POST, verifies that the operation stays pending, checks the real
endpoint, formatted quantity/price, signature and caller client ID, then releases
HTTP and reads the exact string ID beyond the safe integer range. Settlement must
not resend. Runtime and terminal HTTP provide primitive host ports; factories, signer,
formatter, class dispatch and Store are production implementations.

`scripts/order-batch-controls.py` independently disconnects the five production
helpers. Each fault must compile and fail the named real binding assertion;
exact original source restoration must pass after every fault. Compiler failures
or empty test selections do not qualify as controls.

Run from `sdk-rs`, serially:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" \
  node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-binance --jobs=1 --measure --module=order_batches
python3 safe-cex-venue-binance/scripts/order-batch-controls.py
```

Inspect every scoped receipt; measure exit zero alone is insufficient. Full
compiler discovery, macro provenance and dependency guards remain enabled.
Other Binance owners, aliases and feature combinations remain separate work.
