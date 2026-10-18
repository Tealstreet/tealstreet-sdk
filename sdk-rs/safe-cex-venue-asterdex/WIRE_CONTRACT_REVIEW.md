# Private Wire decoder contracts

The 42 local `Wire` declarations in `types.rs` and `builder.rs` now have private
module names. Their derives, fields and public decoding operations are preserved;
module scope allows the compiler inventory to name each actual decoder.

The fixed cohort is executed with:

```sh
CARGO_BUILD_JOBS=1 node sdk-rs/safe-cex-venue-asterdex/scripts/accepted-wire-contracts.mjs
```

Each owner has a real Deserialize shape contract and direct native observations.
The 607 fixed observations execute owned Value, borrowed Value, JSON text and
qualified text decoding, compare every field, retain binary64 bits, and separately
check its public wrapper. Ordinary internal Serde sequences and tagged enum maps
are covered as internal inputs; public map/string restrictions remain separate.
There are no non-decoder exemptions or replacement constructors in this cohort.

Before the move, all 42 production local owners compiled and failed their named
unclassified-Deserialize assertions on master after #854. The cohort runner
requires exactly the 42 compiler owners and every declared field. The ordinary
accepted-set matrix also executes these contracts; the focused runner does not
classify other owners, aliases, features or Wasm.
