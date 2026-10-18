# Spot map visitor evidence

The public Spot `WsFill`, `WsUserFunding`, `WsLiquidation` and
`WsNonUserCancel` decoders each contain a private `OnlyMap` visitor. Their
authority is `hyperliquidspot.types.ts:207-242` (the repeated funding interface
has the same members). The perpetual `types::WsFill` is a distinct owner and
does not establish the Spot visitor's behavior.

Each new contract classifies its real owner as a map decoder. The existing
source scanner delegates the nested visitor to that owner's direct native
observations. No local-function exception, alias policy, decoder bound, source
scanner or production body changes are needed.

The observation calls actual public Deserialize through raw text, raw slice
and a qualified text deserializer. It reads every decoded string's `Vec<u16>`,
each numeric member's binary64 bits and the boolean member directly. It never
serializes a decoded model to derive expected fields. The existing independent
`spot-models-oracle.cjs` supplies four typechecked positive profiles per owner:
distinct field values, empty strings and signed zero, lone UTF-16 units and
numeric overflow, then Unicode and underflow. Authored extra cases reject a
complete declaration-order array and every omitted, null or incorrectly typed
required field. A valid last duplicate overrides an invalid first occurrence;
an unknown lone-unit property is ignored. Raw decoder evidence makes no claim
about Realm class or transport reachability.

From `sdk-rs`, with `CARGO_INCREMENTAL=0`:

```sh
node safe-cex-venue-hyperliquid/scripts/spot-decoder-contracts.mjs
python3 safe-cex-venue-hyperliquid/scripts/spot-decoder-controls.py
```

The first command obtains fresh rustdoc and Cargo compiler artifacts, verifies
the actual macro dependency origins, then uses the maintained inventory,
source scanner, helper guard and `runProbes` engine. It selects the four exact
Spot owners and keeps every local declaration in their shared source file. It
requires four executed owner receipts, four exact delegated visitor receipts,
and zero failures; a measurement process exiting successfully is insufficient.
`--expect-missing` proves the original absent-classification receipts before
adding the contracts. The controls independently corrupt each actual
visitor's returned native fields, require the corresponding named native
observation mismatch, then restore the exact original source and require all
four owner receipts to pass. Compiler failures never count as controls.

The ordinary whole-package `accepted-set.mjs --inventory` gate is unchanged.
Its earlier refusal on the unrelated Realm `facade_store::selected` helper is
owned by the Realm consolidation lane; a finite Spot receipt does not classify
that helper or grant a package-wide exception.

The original source oracle and `tests/spot_models.rs` remain unchanged and run
alongside the new canonical owner observations. Earlier contracts retain their
original bytes, including signed-zero tokens and Unicode escapes.
