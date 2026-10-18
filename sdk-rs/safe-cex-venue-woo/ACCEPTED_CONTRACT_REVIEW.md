# Accepted string-literal declarations

The focused runner reviews 31 remaining concrete V3 enum decoders against the
literal declarations in `packages/safe-cex/src/exchanges/woo/`. Each owner has
its own unchanged contract in the WOO accepted-set contract shard.

| Rust module | Owners | TypeScript reference |
| --- | ---: | --- |
| `v3_contracts` | 12 | `woo.v3.contracts.ts` |
| `v3_account_writes` | 2 | `woo.v3.account-writes.ts` |
| `v3_private` | 2 | `woo.v3.private.ts` |
| `v3_private_orders` | 7 | `woo.v3.private.ts` |
| `v3_regular_writes` | 5 | `woo.v3.regular-writes.ts` |
| `v3_algo_writes` | 3 | `woo.v3.algo-writes.ts` |

The unused v3_private account-mode enum is retired with its account-info decoder.
Only that owner's contract and runner entry are removed. Static registration
checks confirm the remaining 31 owners, 138 valid tags and 617 cases. Positive
strings, forbidden tagged-null maps, invalid kinds and unknown/padded tags retain
all four original Deserialize observations: owned, borrowed, qualified and text.
The text corpus covers ordinary UTF-8, without a lone-surrogate claim.

The runner retains full compiler inventory, Cargo-artifact-authenticated macro
origins, selected-module source scanning and refusal of omitted methods or trait
entrypoints. It uses the canonical probes for this finite cohort, without claiming
whole-package accepted-set clearance or enabling inert V3 routes. The copied-source
control still removes every remaining owner's string guard and requires a named
native variant mismatch for each one; original source bytes must stay unchanged.

Run from sdk-rs with rustdoc artifacts separated from the ordinary target cache:

```sh
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs --control
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs --target=wasm32-unknown-unknown
```

The original 32-owner batch passed native and wasm production probes and compiled
all 32 guard-removal controls to named refusals, then passed restoration. Those
historical runs included the now-retired account-mode owner. They are not relabeled
as a new 31-owner runtime/control execution; this retirement verifies the preserved
contract records and scoped compiler/behavior checks separately.
