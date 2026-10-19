# Live order tables and mapper review

This review reconciles assigned FIX-audit rows 3, 4, 5 and 21 with the
constructed WOO exchange. Their outcome is ACCEPT: the historical typed-table
refusal is absent from the current production binding. This is an evidence
refresh, not a newly repaired production divergence.

## Three mapping tables

`woo.types.ts` exports ordinary objects for ORDER_TYPE, ORDER_SIDE and
ORDER_TRIGGER_TYPE. Unknown keys can therefore resolve to Object.prototype
functions or the prototype object. Those inherited results are observable source
behavior, not permission to replace the tables with closed enum lookups.

`constants.rs` supplies ordered own entries to `binding/globals.rs::enum_table`.
That assembler installs the current realm's Object.prototype. The actual mapper
and inverse-table consumers read these runtime objects, rather than searching the
Rust slices for an input code. The slices describe own entries only; inherited
keys must not become own entries in them.

`live_order_mapping_tests` exercises the actual constructed exchange. Its focused
table witness compares all own keys and values with the maintained TypeScript
constant oracle, checks inherited values are absent from own keys, and verifies
shared referent identity across all three tables and their mapper outputs.
The limit/algo mapper corpus also covers all twelve inherited prototype names,
unknown codes and declared codes. No native output is projected to JSON before
its function/object identity, own presence, strings or number bits are inspected.

## mapLimitOrder

The source body at `woo.exchange.ts:1832` and the production runtime mapper in
`binding/hosts/order_mapping.rs` use the same truthy ID, quantity and price
fallbacks, strict status/time-in-force tests, ordinary table lookup and shared
subtraction. The result keeps own undefined fields, copied runtime values and
the original row as info. Symbol/market admission precedes mapping; each accepted
order observes the clock at the same point.

The maintained-source oracle checks all 103 output records, 36 native numeric-bit
profiles and 12 algo timing/error profiles. The existing constructed-member tests
compare every own output field and original info identity against those fixtures.
The focused table test additionally mutates the original limit-order input after
mapping and observes that mutation through info.

Acceptance concerns the runtime mapper and table lookups. Public typed Order
admission and Store field domains remain separately governed by shared-type
decision 3; a direct mapper call does not claim that arbitrary functions can be
inserted into typed Order fields. No shared type or production TypeScript changes.
