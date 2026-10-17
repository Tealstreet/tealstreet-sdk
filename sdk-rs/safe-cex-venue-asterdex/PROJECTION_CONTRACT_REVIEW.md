# Twenty function contracts

This batch reviews twenty original native/default census functions. It adds no
production behavior or policy permissions. The maintained TS bodies remain the
behavioral reference. These selected witnesses do not settle aliases, receivers,
local declarations, feature/wasm configurations, or the whole package.

The accepted rows live in `accepted-set/contracts/safe-cex-venue-asterdex.json`.
`scripts/accepted-projection-source.cjs` executes maintained TS declarations or
AST-selected members against 274 independently authored input/output expectations.
`scripts/accepted-projection-contracts.mjs` obtains fresh native/default rustdoc,
checks exact selected compiler owners and API identities, and runs actual compiled
production-function probes. No classification-only receipt is accepted.

| Original owners | Reviewed observations |
| --- | --- |
| `api_helpers::{build_v3_nonce,build_aster_dex_v3_nonce}` | Per-signer monotonic state, repeated/backward/zero clocks, scope isolation, exact large integers and BigInt refusal before state mutation. Each owner/case uses a distinct scope. |
| `builder::build_aster_approve_builder_typed_data` | Every domain/type/message field, both Aster chains, absent/empty/non-ASCII names and a string nonce beyond 2^53. |
| `builder::{is_aster_maker_only_order,get_aster_builder_fee_rate_for_order}` | Native order/time-in-force enum inputs, post-only alternatives, API-type conjunction, maker/taker rates and exact-zero omission with `0.0` retained. |
| `builder::{resolve_aster_builder_fee_config,get_aster_builder_approval_config}` | Every configuration field, normalization, default/custom addresses, testnet placeholder, error ordering, zero-fee early return before address validation and maximum approval fee. |
| `exchange_helpers::{parse_stp_mode_response,build_change_stp_mode_request}` | Native V0/V1/V2 identity plus wire value, invalid input errors, fallible String conversion and complete validated POST effect. |
| `exchange_helpers::build_change_margin_type_request` | Missing/empty symbol refusal, whitespace/non-ASCII preservation, both API versions and both margin types. |
| `exchange_helpers::{get_fapi_path,get_account_from_alias}` | Explicit version override, leading-slash-only trimming, literal query spelling, alias/key/empty fallback and all actual native account fields. |
| `ws_helpers::public_connection_url` | Both actual source endpoints and literal query concatenation for fixture-supplied request IDs. |
| `exchange_helpers::transform_order_book` | Stable side-specific sorting, all level fields, cumulative price-times-amount totals, duplicate prices, parseFloat tails, negative zero and non-finite results. |
| `exchange_helpers::balance_from_account_response` | All native balance fields, truthy fallback before parseFloat, margin addition order, negative zero/infinities/NaN, conversion failure and absent metadata/options. |
| `exchange_helpers::{reverse_map_order_status,reverse_map_order_type}` | Every mapping, fallback, all twelve stock prototype keys, native TableHit/ProtoMember identity and actual TS referent identity before JSON projection. |
| `exchange_helpers::extract_filter_value` | First matching filter, null-before-match error, duplicate short-circuit, truthiness/coercion, inherited fields and native numeric results. |
| `exchange_helpers::collect_open_interest_from_payload` | Root/data/data-array admission, Number conversion, finite filtering, symbol type/emptiness, insertion-order overwrite and mutations retained before a later error. |
| `account_helpers::parse_order_number` | Actual TS `filled`/`amount` initializer expressions, bounded fixture fields `z`/`q`, truthy fallback, parseFloat, signed zero/non-finite results and conversion refusal. |

Numeric projections observe the actual native values before JSON serialization.
Finite values, signed zero and infinities retain their binary64 bits. NaN is
classified explicitly rather than pinning an engine-dependent NaN payload/sign.
Option presence, native enum variants, inherited capabilities, insertion order and
partial writes are observed separately where serialization would erase them.

The source runner uses actual ethers for builder-address behavior. Clock readings,
transport response/submission, logging and nanoid output are explicit dependency
fixtures. STP caller feature gates and transport submission remain separate from
the pure helper owners. The private numeric helper is compared to the two actual
TS lexical initializers, without rewriting their parsing algorithm in the oracle.

## Compiled discrimination controls

`scripts/accepted-projection-controls.py` compiles each production mutation in
isolation. It requires exactly the named failed native owners, behavioral output
mismatches, and genuine executed receipts; compiler errors cannot satisfy a
control. Each original source byte sequence is restored in `finally`, then all
twenty native owners must pass before the next mutation starts.

| Isolated production mutation | Required failing owners |
| --- | --- |
| Nonce greater-than becomes greater-or-equal | Both nonce functions |
| Maker limit-type condition inverted | Maker decision and order fee selection |
| Both-zero fee test becomes either-zero | Configuration and approval configuration |
| Book side sorting reversed | Order-book projection |
| Every inherited status becomes the ToString referent | Reverse status lookup |
| Collector swallows Number coercion error | Open-interest collector |
| Falsy order-number fallback becomes negative zero | Private order-number helper |
| Typed-data Aster chain inverted | Builder typed data |
| STP V0 becomes V1 | STP response parser |
| Isolated/cross margin type inverted | Margin request builder |

Measured results: actual TS passed all 274 expectations across twenty owners;
fresh native/default production probes produced twenty runtime receipts, zero
classification-only receipts and zero failed owners. All ten isolated compiled
faults failed exactly their named owners (2, 2, 2, then seven single-owner faults).
All ten exact restores passed all twenty owners. Compiler errors were refused as
control evidence. Per-run logs and JSON receipts were retained in the local
`asterdex-projection-controls-8eo4gtpx` directory.

Reproduce with the provisioned TypeScript oracle dependency path and Rust toolchain:

```sh
node sdk-rs/safe-cex-venue-asterdex/scripts/accepted-projection-contracts.mjs
python3 sdk-rs/safe-cex-venue-asterdex/scripts/accepted-projection-controls.py
```

After merging master `41e20791f8` (including #380), frozen dependency installation,
actual TS 20/274, native twenty runtime / zero classification-only / zero failed
owners and `parity:check` all passed. Parity measured 7459 rows and 17214 public
Rust items, with zero findings. The twenty selected function bodies, contracts,
source observations and control targets were unchanged by the master refresh.

The leverage-bracket response helper remains outside this batch: its separate
key-identity repair is PR #380, and its array-like input behavior needs independent
review. The original census still has 95 Aster owners after these twenty are
accepted; the eight original aliases were reviewed separately. No remaining owner
is exempted by these selected contracts.
