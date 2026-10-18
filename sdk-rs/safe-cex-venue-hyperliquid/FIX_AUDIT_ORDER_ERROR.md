# Unreadable order-response Error audit

The historical class row described only the portable `order_response.rs`
`Result` adapter. The constructed production exchange already installs
`realm/imports_venue.rs::unreadable_error_class`: a subclass of the same Error
intrinsic used by the bound exchange, with a stable constructor and prototype.
The parser reconstructs its `Unreadable` result with that class. The placement
consumer tests `instanceof` that class, schedules reconciliation, and rethrows.
Venue rejections use plain Error and do not enter that reconciliation branch.

The old claim of missing Error inheritance is superseded at this production
boundary. This audit changes no production behavior, heap implementation or
shared guard. The standalone portable adapter continues to expose typed Rust
results; it is not an executable JavaScript exception object.

`binding::order_error_audit` constructs the real `HyperliquidExchange` with the
shared native test runtime and its production imports and class methods. Four
constructor profiles compare name, message and display UTF-16 units, both
`instanceof` checks, constructor and prototype identity, superclass links,
name/message descriptors and enumerable keys with the maintained TS class.
The messages include empty text, an astral character and an unpaired surrogate.

Fourteen actual bound `validateResponseAndExtractOrderIds` calls cover empty
bodies, missing/empty statuses, unsupported/unrecognized statuses, two venue
refusals, successful resting/filled IDs and both deferred statuses. The original
parser and catch branch execute unchanged. Only `scheduleOrderRefresh` is
replaced with a counter, so its call count is observable without running later
network hydration. Nine unreadable responses throw the custom Error and request
one refresh each; two venue refusals throw plain Error with no refresh; three
accepted responses return exact IDs without this catch-branch refresh.

`scripts/order-error-audit.cjs` freshly loads the complete maintained response
module and extracts the unique response-consumer method from the maintained
exchange class using the TypeScript AST. Both execute in the same Node VM;
the class, parser, Error intrinsic and `instanceof` are real. The source class
is wrapped only with the same counting refresh hook as the native probe.
No golden results or handwritten parser/consumer model supplies expectations.

From `sdk-rs`, with `SAFE_CEX_TYPESCRIPT_PATH` pointing to the installed package:

```
CARGO_INCREMENTAL=0 cargo test --locked --jobs 1 -p safe-cex-venue-hyperliquid --lib binding::order_error_audit::
CARGO_INCREMENTAL=0 python3 safe-cex-venue-hyperliquid/scripts/order-error-audit-controls.py
```

The controls independently corrupt the actual constructor name, replace the
parser's custom Error with plain Error, and invert the actual consumer's
`instanceof` branch. Each must compile and fail a named native assertion; exact
source restoration must pass both tests. This is a regression-proof audit of an
existing binding repair, not a claim that a new production fix was necessary.
