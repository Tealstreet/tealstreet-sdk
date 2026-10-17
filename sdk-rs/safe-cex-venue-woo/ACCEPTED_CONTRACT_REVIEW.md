# Accepted string-literal declarations

This batch reviews 32 concrete V3 enum decoders against the literal declarations
in `packages/safe-cex/src/exchanges/woo/`. Their individual contracts are in
`../safe-cex-parity/accepted-set/contracts/safe-cex-venue-woo.json`.

| Rust module | Owners | TypeScript reference |
| --- | ---: | --- |
| `v3_contracts` | 12 | `woo.v3.contracts.ts` |
| `v3_account_writes` | 2 | `woo.v3.account-writes.ts` |
| `v3_private` | 3 | `woo.v3.private.ts` |
| `v3_private_orders` | 7 | `woo.v3.private.ts` |
| `v3_regular_writes` | 5 | `woo.v3.regular-writes.ts` |
| `v3_algo_writes` | 3 | `woo.v3.algo-writes.ts` |

The contracts list all 140 valid tags independently and match decoded Rust
variants directly. Each tag has a positive string case and a rejected tagged-null
object case. Additional cases reject null, booleans, numbers, arrays, objects,
unknown tags and whitespace-padded tags. The 632 cases execute four actual
entry paths each: owned Value, borrowed Value, qualified Deserialize and JSON
text. The text corpus contains ordinary UTF-8 strings; it makes no claim about
lone-surrogate strings that a serde_json Value cannot carry.

The original native run passed the twelve existing strict `v3_contracts` owners
and failed the other twenty owners because their derives accepted tagged maps.
Those twenty now use the shared [string-only enum recipe](../safe-cex-parity/accepted-set/MAP-ONLY-INGRESS.md):
String admission followed by a private remote companion. Existing Serialize,
variant names, wire renames and numeric-output implementations are retained.
The private-order module also replaces its protocol glob with the sixteen
actual imported names so source discovery can authenticate its attributes.

The maintained focused runner first performs ordinary compiler inventory of the
whole package, then requires every selected enum to have no omitted methods,
conversions or trait entrypoints. Actual Cargo artifacts authenticate its macro
origins, and the source scanner still checks all six selected modules. It runs
the standard production probes for these 32 owners; it does not claim acceptance
of the remaining owners or aliases. Current discovery retains 645 owners,
ten aliases and nine separately reviewed construction obligations.

Run from the repository root:

```sh
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs --control
RUSTC_BOOTSTRAP=1 node sdk-rs/safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs --target=wasm32-unknown-unknown
```

The control changes only copied production sources after contract validation:
it removes each concrete string frontdoor and invokes that owner's remote
schema directly. All 32 owners must compile and fail their named native
variant observations when tagged maps enter. A normal run restores the strict
source behavior. Original production bytes are checked after every run.

Verified results for this batch: all 32 production runtime receipts pass on the
native target and on `wasm32-unknown-unknown`, with no explicit classifications
or failed owners. The copied-production control compiles and fails all 32
owners, with 140 named tagged-map variant mismatches; exact native restoration
passes again. The runner verifies real production bytes remain unchanged;
ordinary source and contract guards stay active.
