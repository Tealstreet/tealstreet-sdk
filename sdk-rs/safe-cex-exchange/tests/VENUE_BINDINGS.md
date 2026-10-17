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

`venue-binding-master-evidence.json` retains the initial master source evidence
at `999f0564c2`; it is an audit artifact, not an acceptance list. Browser-guarded
and unreachable error branches are included as static leads, not asserted bugs.
The Paradex intrinsic dispatcher delegates unknown names to its registered
globals, so its `parseFloat` is bound. Lighter's private class constants and
`super.dispose` resolve through their production receiver/superclass tables.
Hyperliquid uses its own realm rather than the shared global installers.

Set `VENUE_BINDING_INVENTORY=/tmp/venue-bindings.json` for current file/function
evidence. `VENUE_BINDING_BASE_REF` selects an already-fetched base commit; local
runs default to the merge base with origin/master. The initial gate PR alone
bootstraps the baseline when the base commit has no baseline file.
