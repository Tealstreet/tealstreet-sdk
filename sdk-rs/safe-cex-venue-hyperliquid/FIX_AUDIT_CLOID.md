# Cloid FIX-audit reconciliation

This cohort covers `HYPERLIQUID_CLOID_PATTERN` and
`placementRecordFromWire` from the de-duplicated codex-3cmqhi assignment.
It does not change the family Realm or its scheduling and Store bridges.

## HYPERLIQUID_CLOID_PATTERN: ACCEPT the representation difference

The maintained source exports a non-global, non-sticky regular expression;
the native module exposes its display and a string predicate. The production
class import provides `.test`. The source consumers in `clientOrderId.ts` and
`HyperliquidExchange.findOrderByClientOrderId` use pattern display/validation,
not `lastIndex`, flags, RegExp prototype identity or mutation. The package entry
point does not export the cloid module. Rust's registry, FFI and wasm surfaces
expose the typed client-ID rule and lookup, not the private Realm import object.

`binding::cloid_audit::cloid_audit_pattern_controls_the_exported_lookup_request`
constructs the actual `HyperliquidExchange`, invokes its exported lookup and
counts actual REST requests. Only the valid lowercase ID reaches `orderStatus`;
uppercase, trailing-newline and short IDs produce no request. The existing
34-case maintained-source oracle separately checks pattern display, validation,
read/rule/mint helpers and record fields. The pattern has no stateful g/y flag,
so the supported callers cannot observe missing mutable RegExp state.

This accepts the mapped representation difference, not arbitrary RegExp or
JavaScript reflection equivalence.

## placementRecordFromWire: ACCEPT the nonboolean input-domain difference

TS copies `wire.r` verbatim even if its declared boolean `OrderWire` type is
bypassed. The standalone Rust helper rejects nonboolean `r`. Both observations
are executed explicitly in the oracle/native probe; the difference is retained.
Source `orderRequestToOrderWire` writes `r: order.reduce_only`, and production
placement builds that boolean from placement options. The Rust placement API
likewise accepts typed placement options (`PlaceOrderOpts.reduce_only: Option<bool>`); registry, FFI and wasm have no entry
point accepting an arbitrary cloid record wire. Incoming remote order frames
use their separate order mapper, not `placementRecordFromWire`. Native
`limit_orders` and `placement_dispatch` apply `Boolean` (or a literal true)
before `resolveReduceOnly`; that method returns false for Spot and the requested
boolean otherwise. The source has the corresponding call chain.

`cloid_audit_generated_wires_reach_the_real_placement_cache` constructs the real
exchange and uses its installed `orderRequestToOrderWire`, `recordPlacement` and
`getPlacementRecord`. Twelve profiles cover both reduce-only flags, both sides
and limit/TP/SL. All eight record members, including native price/amount/time
bits, match the maintained TS helper on those generated wires. The probe checks
that the actual producer writes a boolean before reaching the helper. It does
not replace that helper, cache or class method with a test callback.

The integration test `binding::cloid_audit_exported_placement_produces_boolean_record_inputs`
also starts the real exchange and calls `BaseExchange::place_order` for eight
limit/stop, buy/sell, reduce-only combinations. It observes successful signed
HTTP requests, generated cloids and exact boolean `r` fields.

The exported placement path cannot supply the nonboolean input at issue. This
does not assert equivalence for an arbitrary manually forged `serde_json::Value`
or for calls to source public methods outside the native Exchange interface.

## Reproducible consumer controls

From `sdk-rs`, with `CARGO_INCREMENTAL=0` and the installed TypeScript oracle:

```
cargo test --locked --jobs 1 -p safe-cex-venue-hyperliquid --lib binding::cloid_audit::cloid_audit_
cargo test --locked --jobs 1 -p safe-cex-venue-hyperliquid --test cloid
python3 safe-cex-venue-hyperliquid/scripts/cloid-audit-controls.py
```

The controls change the actual native pattern prefix and placement boolean
mapping separately. Each must compile and fail its named constructed consumer
assertion, then pass after exact source restoration. No production behavior
change, decoder classification, exemption or shared guard change is required.
