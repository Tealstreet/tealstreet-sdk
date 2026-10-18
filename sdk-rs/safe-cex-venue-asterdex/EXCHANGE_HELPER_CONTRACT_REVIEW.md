# Exchange helper accepted contracts

The twelve `exchange_helpers` functions below are production compiler owners.
Each contract calls that function directly; none receives a placeholder
non-decoder classification. The scoped runner rediscovers their private/public
Rustdoc identities and inserts observations beside the real production bodies.
The full accepted matrix discovers and executes the same contract rows.

- `is_raw_algo_order`, `get_open_algo_orders_count` and
  `assert_algo_order_capacity`: uppercase raw kinds, typed kinds, symbol/status
  filtering, exact capacity, disabled limits and visible coercion errors.
- `parse_leverage_brackets_response`: insertion order, duplicate replacement,
  absent values, retained prefixes after failure, and warning versus info logs.
- `transform_market`, `transform_ticker` and `transform_position`: native numeric
  bits, precision and leverage defaults, matching realized PnL, source coercions,
  and the approved shared numeric-limit conversion.
- `map_public_trade`: both uppercase source directions remain visible closed
  `OrderSide` refusals. The contract does not invent lowercase values.
- `retained_info`: signed zero/infinity overwrite bits, unrelated metadata,
  absent optional overwrite and missing-pointer refusal. This is a Rust-only
  serialization-boundary helper, not a claimed extra TypeScript API.
- `after_method_called_in_worker`, `assert_v3_feature` and
  `fetch_open_interest_map`: actual setting effects and rejection, actual
  BaseExchange error emission, bulk/fallback request order, timeout and logs.
  The adapter fixtures provide only external setting/runtime/HTTP ports. They
  never implement the production conditional, mapper, fallback or guard body.

`accepted-exchange-helper-source.cjs` executes maintained TypeScript methods for
41 exact cases and separately checks three documented shared-output boundaries.
Four retained-pointer invariants are checked directly in native Rust. The 48
fixed expectations were authored independently of production Rust output.
Native observations inspect values/bits before owner serialization; HTTP test
replies are encoded only at the actual backend response boundary.

Scoped execution:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript \
  node sdk-rs/safe-cex-venue-asterdex/scripts/accepted-exchange-helper-contracts.mjs
```

This is a twelve-owner native check, not a complete Aster accepted-set pass.
Other functions, alternate owners and function-local `Wire` owners keep their
separate full-matrix obligations. Placement input types and the canonical
[string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[closed output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) and
[owned snapshot](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias)
boundaries are unchanged.
