# Production venue binding ratchet

`cargo test -p safe-cex-exchange --test venue_binding_gate` scans the production
Rust module graph for named global/import/intrinsic requests, follows local
lookup wrappers, and compares them with production registrations. Test-only
modules/functions cannot satisfy a production request. Literal registrations,
registration closures, object members, tables and class installers are inspected;
this is a syntax gate, not Rust type inference or a reachability proof. New host
registration idioms must extend the scanner and its controls, not add waivers.

`venue-known-unbound.json` is the explicit debt baseline. Any newly unbound name
fails. A fixed name left in the baseline also fails: the owning venue PR must
remove it. CI compares the baseline with the fetched PR base and rejects growth,
even if a PR adds the same missing name to both code and baseline.

The initial master inventory at `999f0564c2` was reported to the venue lanes.
Browser-guarded and unreachable error branches are included as static leads,
not asserted bugs. Inventory reports stay outside the repository.
The Paradex intrinsic dispatcher delegates unknown names to its registered
globals, so its `parseFloat` is bound. Lighter's private class constants and
`super.dispose` resolve through their production receiver/superclass tables.
Hyperliquid uses its own realm rather than the shared global installers.

Set `VENUE_BINDING_INVENTORY=/tmp/venue-bindings.json` for current file/function
evidence. `VENUE_BINDING_BASE_REF` selects an already-fetched base commit; local
runs default to the merge base with origin/master. The initial gate PR alone
bootstraps the baseline when the base commit has no baseline file.

The scanner follows Lighter binding/function-value lookups and captured module
lookups. Captured paths preserve their host namespace: OKX/KuCoin distinguish
`this` from globals; Bitget captures receiver paths and dispatches imports through
its operation table; Polymarket selects the `imports` receiver explicitly;
Lighter's capture hosts declare IMPORTED_CALLEES and MODULE_BINDINGS. Two-argument
socket captures select receiver fields and members. Intrinsic registration
comes only from production intrinsic/operation dispatchers, not unrelated matches.
Test-gated impl blocks are excluded alongside test modules and functions.

Shared install_library adds Number.parseInt and virtualClock.getCurrentTime via
define_on; these are bound. Five Hyperliquid market-bootstrap timeout reads go
through an imported class and are not free module bindings. These initial scanner
false positives were corrected before landing the seed baseline.

With `--all-features` on a native host, the method gate also scans literal standard
JS/Dayjs method requests, including forwarded lookup wrappers, closure-field host
APIs and macros. It reads callable availability from actual production
NativeRuntime/ScopeShared values with installed shared builtins and library;
test hosts never supply the implementation surface. Hyperliquid's separate realm
is excluded from this shared-runtime check.

`venue-standard-methods.json` recognizes names from Node and the installed source
Dayjs/BigNumber packages. Regenerate with
`node sdk-rs/safe-cex-exchange/tests/venue_method_gate/catalog.cjs`; it may only
expand relative to the PR base, preventing removal from hiding an unsupported
name. `venue-known-unbound-methods.json` follows the same shrinking ratchet as
the global/intrinsic baseline. Set `VENUE_METHOD_INVENTORY` for source locations.

This method check is deliberately a name inventory, not receiver-type inference:
a callable with the same name on another builtin family can mask a missing
receiver-specific method. It does not prove semantics, dynamic keys or runtime
reachability. Shared method fixes still require Node/source verification and
production-runtime regressions; add new lookup idioms and controls to this gate.

`python3 sdk-rs/safe-cex-exchange/tests/venue_method_gate/controls.py` runs two
compiled production faults sequentially: remove Array `push`, then remove the
shared Math registration. Each must fail its named inventory assertion, restore
exact source bytes and pass again. Logs default to a temporary directory.
Run it without concurrent source readers or compilers in this worktree.

Negative syntax controls exclude function debug names/callback results,
test-only constants, match arms and inner file cfg, multi-hop alias loss and tuple-helper
shadowing across lexical blocks. Tuple-pair helpers are recognized from their
actual closure bodies rather than their names.
