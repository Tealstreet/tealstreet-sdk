# Shared option-field audit (s75354-24)

The shared fields already exist in the model, structural Wire decoders and Num
output census. This audit checks their construction and downstream retention,
then repairs the shared account/Store paths that lost them. No venue fields are
invented where the maintained source omits them.

## Construction inventory

At base `e067b3b8729c`, a Syn
ExprStruct walk over the Rust sources found 53 expressions named Market or
Position. Of these, 46 are outside test modules/files: 42 construct shared models
(15 Market, 27 Position), three are Backpack-local raw record constructors, and
one is a Hyperliquid predicate enum variant. Seven remaining expressions are test
fixtures. Function return types, declarations and patterns are not construction
sites. The historical queue's 76/83 estimates are not used as measured counts.
The named-expression inventory is supplemented by source JS/JSON producers below;
it does not pretend a syntax walk resolves arbitrary Rust aliases or macro bodies.

| Crate | Shared Market literals | Shared Position literals |
| --- | ---: | ---: |
| `safe-cex-venue-apexomni` | 1 | 2 |
| `safe-cex-venue-asterdex` | 1 | 2 |
| `safe-cex-venue-backpack` | 1 | 2 |
| `safe-cex-venue-bingx` | 1 | 3 |
| `safe-cex-venue-bitget` | 0 | 4 |
| `safe-cex-venue-bitunix` | 1 | 1 |
| `safe-cex-venue-blofin` | 1 | 2 |
| `safe-cex-venue-breakoutprop` | 1 | 2 |
| `safe-cex-venue-bybit` | 2 | 1 |
| `safe-cex-venue-coinbase` | 1 | 1 |
| `safe-cex-venue-grvt` | 1 | 1 |
| `safe-cex-venue-kraken` | 0 | 1 |
| `safe-cex-venue-kucoin` | 1 | 1 |
| `safe-cex-venue-mexc` | 1 | 2 |
| `safe-cex-venue-weex` | 1 | 1 |
| `safe-cex-venue-woo` | 1 | 0 |
| `safe-cex-webworker` | 0 | 1 |

All 41 non-option construction sites set None, matching omission by their source
market/position mapper. The remaining shared literal is
`safe-cex-venue-bybit/src/options.rs::map_bybit_option_instrument`, which already
sets all eight OptionTerms fields. The Backpack shared model bridge correctly
omits OptionTerms. Its unused local raw `option: false` flag was subsequently
removed during the shared-Js cutover because source fetchMarkets omits it; the
actual-source REST oracle now pins that absence.

A source scan of exchange modules finds only two producers of these shared
properties: `bybit.options.ts::mapBybitOptionInstrument` and
`bybit.exchange.ts::mapPosition` for an option symbol. Unrelated WOO subscription
parameters named option are not shared Market/Position metadata. Other appearances
read the metadata or refer to the exchange category string.

Bybit's shared-runtime mapPosition already produces a present PositionOption,
with Greeks when available and an empty object otherwise, and forces leverage=1
and liquidationPrice=0. This path is in binding/members/mapping.rs; it is a JS
object producer and is deliberately counted separately from Rust struct literals.
No venue production code changes are needed for this sweep. The existing
options_inverse test executes actual source option instrument/Greeks helpers;
it is helper coverage, not a concrete venue option-fetch receiver E2E.

## Repaired connected paths

- Account construction now forwards option and inverse opt-ins only when the
  source metadata supports that toggle and the account value is true. Shared
  patches preserve absent, explicit undefined/clear, false and true overrides,
  after metadata gating. Public factory patches carry the same fields.
- Store partial Market/Position patches accept their known option field instead
  of treating it as an unknown key. Their merges retain old metadata when the
  incoming typed row omits it, matching source Object.assign/in-place patching.
- JS Store option patches use the existing native ModelPatch field carrier, so
  nested expiry/strike/Greeks keep NaN, infinities and signed zero. Other patch
  fields keep their independently documented representation profile.
- JS Store.update forwards the source fromWorker argument. Worker market refresh
  therefore takes the existing in-place merge path and retains omitted metadata.
  Ordinary venue updates continue to use the source default false.

## Evidence

`safe-cex/tests/root_option_toggles_source.cjs` supplements the unchanged original
root-services feeder. It evaluates the actual accountToExchangeOptions body and
its maintained metadata/routing collaborators. One Node process covers all 48
venues x three toggles x three account states x four override states = 1728
cases. Rust drives each through create_exchange_from_account to the supplied
constructor, asserting exact toggle presence/value and metadata warnings.

`safe-cex/tests/option_store_source.mjs` executes the actual Store over five
numeric inputs (2.5, -0, NaN, +Infinity, -Infinity). Eight ordered mutations per
input cover market patch, single/batch position patches, single/batch/full
position merges, worker market refresh, and a present empty PositionOption.
The native test uses the production DefaultStore through its JS facade and
compares original native number tags and complete option fields before any JSON
wire projection. The test failed before the repair: all metadata patches were
ignored. No alias-identity parity claim is added by the facade snapshot reads.

Required scoped commands:

```sh
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex --test root_services --test option_fields --test production_store_wiring
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bybit --test options_inverse -- --include-ignored
```

The shared typed-patch, owned-snapshot and UTF-8 choices remain declared in
MANIFEST-SPEC. Present malformed option objects still reject at that typed
boundary. This audit does not claim every venue exposes option trading or close
unrelated representation/option-consumer findings.
