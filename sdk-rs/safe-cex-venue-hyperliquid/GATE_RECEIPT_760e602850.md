# Builder/constants leaf receipt

**Moved19 pending→ported, 0 pending→divergent**, independently measured with
tomllib against integration dab5ce9827:66/106/359 becomes85/106/340.
All531 manifest keys remain;19 movements are listed below. The sweep stays OPEN.

Measured code SHA: `760e602850c4bbc40d4a7db9414d7744d7448ce3`.
Actual direct parent built on: `dab5ce98271927006e7a34e759c331372991d39b`.
Branch: `feat/hyperliquid-builder-tiers`. One implementation commit above the
requested integration parent; sub-agent review found no remaining blocker.
The later evidence-only commit is not claimed as newly tested.

This slice ports the core builder/constants leaf and spot subscription constants.
UTF-16 address values retain unpaired surrogates. Such addresses cannot match an
ASCII hex allowlist entry after JS trim/lowercase, so query predicates return the
source None/false result instead of an ingress refusal. Configuration fields
retain UTF-16 units and native f64 bits, including signed zero and NaN.
Configured resolution returns Cow::Borrowed to the original native configuration;
fallback is fresh Cow::Owned. No JSON configuration codec is claimed.

Review found a real typed-getter gap in the initial eager resolver, supported by
an actual-source strict typed witness. It was fixed before commit: the fallible
HyperliquidBuilderProvider reads builder once, skips testnet when configured,
and propagates its generic error without conversion. Five source getter cases
pin read order, errors and early return. There is no surviving getter finding.
Model value representation and the provider/options ABI are mapped to existing
Rust symbols; no source row or target is fabricated.

| Gate | Exit | Measured scope |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid --all-targets --jobs 2` | 0 | Scoped host compile floor |
| Exact Yarn1.22.21 `parity:extract:check` | 0 | Output inspected:0 manifest updates /0 added /0 orphaned,7341 symbols /512 source files |
| Combined constant mutation, exact constants test | 101 | All13 roots individually reach named native assertions; panic capture continues through all roots and final failure list contains each name in order |
| Six focused function/model controls | 101 each | Network, fee lookup, paired allowlist, fee-rate text, native model fee input, getter read order |
| Restored `cargo test ... --test builder_constants --jobs 2` | 0 | 3 passed /0 ignored; all production/test source files restored to committed bytes |
| `python3.12 scripts/audit-row-dispositions.py --check ROW_DISPOSITIONS.json` | 0 | 85/106/340; all earlier63 roots and13 original finding names retained individually |
| Independent tomllib movement census | 0 |19 pending→ported,0→divergent,531 keys in both trees |

Cargo calls were sequential, with `--jobs 2`, never concurrent. Source oracles
used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
The Node program executes one complete corpus per binary via OnceLock:13 native
constant roots,105 data function cases (2 network,38 fee lookups,43 paired
allowlist,13 fee texts,9 configurations), plus5 getter cases. That is the
iteration floor, not an exhaustive runtime object or workspace verdict.

The code was committed before these gates. Mutations were injected into that
SHA's production/test consumers and restored in finally; the restored full test
target passed. Each101 result required compilation reaching `running 1 test`,
its named assertion, and `test result: FAILED.`. No compile-only failure counts.
The writer was run after manifest edits and canonicalized the resolver rs array
(one manifest updated,0 added/orphaned), before this measured code SHA.

## Discriminating controls

```text
SHA 760e602850c4bbc40d4a7db9414d7744d7448ce3
all13-constants exit 101 BUILDER named native constant failures named roots 13
network exit 101 BUILDER native network address named roots 0
lookup-fee exit 101 BUILDER native address fee named roots 0
known-pair exit 101 BUILDER native known address/rate pair named roots 0
fee-text exit 101 BUILDER native rate text 3 named roots 0
model-fee-input exit 101 BUILDER native config fee bits 5 named roots 0
getter-order exit 101 BUILDER native getter read order 0 named roots 0
restored builder tests exit 0: 3 passed
```

All13 constants were deliberately changed in one control run. Each native root
assertion executed despite earlier panics, so the measurement is13 named
assertion failures, not an inference from a first-root failure. This covers the
13 constant rows with one compile/test process. The remaining six controls are
one focused case per newly ported function/model row:

| Control | Concrete change / observed failure |
| --- | --- |
| network | Invert testnet selector; mainnet selects testnet wallet and fails native network address |
| lookup-fee | Add1 to the matched fee; mainnet10 becomes11 and fails native address fee |
| known-pair | Match unequal rate instead of equal; mainnet at0 incorrectly passes, fails native address/rate pair |
| fee-text | Combine /10/100 into /1000;7 changes0.006999999999999999% to0.007%, fails native rate text3 |
| model-fee-input | Test consumer corrupts the decoded configuration fee30 to31; independently captured source fee bits fail at config5. This is native member assertion sensitivity, not a claimed production decoder defect |
| getter-order | Read testnet before builder; builder-error case reads testnet/builder instead of builder only, fails native getter read order0 |

Native static values, output predicates/text, UTF-16 members, fee bits, pointer
identity and callback effects are checked directly. No production record
serialization or decode/re-encode roundtrip supplies these assertions. NaN
captures canonicalize NaN only; finite and signed-zero bits remain exact.
No original feeder bytes were rewritten; this oracle is a new actual-source
whole-module consumer, with no retroactive independence claim.

## Row movements

| Manifest | Symbol | Movement |
| --- | --- | --- |
| hyperliquid.constants.ts.toml | singularSubscriptionTypes | pending→ported |
| hyperliquid.constants.ts.toml | systemSubscriptionTypes | pending→ported |
| hyperliquid.constants.ts.toml | MAINNET_API_URL | pending→ported |
| hyperliquid.constants.ts.toml | TEALSTREET_BUILDER_ADDRESS_MAINNET | pending→ported |
| hyperliquid.constants.ts.toml | TEALSTREET_BUILDER_ADDRESS_TESTNET | pending→ported |
| hyperliquid.constants.ts.toml | getBuilderAddress | pending→ported |
| hyperliquid.constants.ts.toml | TEALSTREET_BUILDER_FEE | pending→ported |
| hyperliquid.constants.ts.toml | HYPERLIQUID_BUILDER_TIERS | pending→ported |
| hyperliquid.constants.ts.toml | hyperliquidBuilderFeeTenthsBps | pending→ported |
| hyperliquid.constants.ts.toml | isKnownHyperliquidBuilder | pending→ported |
| hyperliquid.constants.ts.toml | HyperliquidBuilderConfig | pending→ported |
| hyperliquid.constants.ts.toml | builderFeeRateFromTenthsBps | pending→ported |
| hyperliquid.constants.ts.toml | resolveHyperliquidBuilder | pending→ported |
| hyperliquid.constants.ts.toml | SPOT_ASSET_INDEX_OFFSET | pending→ported |
| hyperliquid.constants.ts.toml | PERP_MAX_PRICE_DECIMALS | pending→ported |
| hyperliquid.constants.ts.toml | SPOT_MAX_PRICE_DECIMALS | pending→ported |
| hyperliquidspot.constants.ts.toml | singularSubscriptionTypes | pending→ported |
| hyperliquidspot.constants.ts.toml | systemSubscriptionTypes | pending→ported |
| hyperliquidspot.constants.ts.toml | MAINNET_API_URL | pending→ported |

Not run: full venue suite, workspace compilation, host clippy, wasm check/clippy,
crypto tests, wiring/layer/parity runtime suites, old-row control matrices, full
monorepo suites, CI rerun or live-exchange calls. No approval/network workflow
or exchange/socket constructor composition is promoted by this leaf. Old
four-class hardening remains secondary/paused; ROW_DISPOSITIONS.json retains
its source-level references and individual findings. The only retained untracked
file is the user's .aimux-priority.md.
