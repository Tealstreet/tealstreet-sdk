# WOO websocket retirement review

Assigned rows 22–45, 54 and 59 are ACCEPT. These declarations belong to the
inactive V3 websocket island, not the legacy public/private sockets constructed
by WOO. Row 46's sequence helper was retired in the preceding batch.

The TypeScript source is `packages/safe-cex/src/exchanges/woo/woo.v3.websocket.ts`.
Its only runtime imports are audit tests; the separate inert ticker and recovery
modules import types. The maintained whole-class dependency probe starts from
`woo.exchange.ts`, `woo.api.ts`, `woo.ws-public.ts` and `woo.ws-private.ts`, loads
no V3 module, constructs two receivers and checks all eight hard-false gates.
The native constructed exchange checks all 47 assigned V3 identities against
its actual receiver and runtime globals, with positive legacy member checks.

A tracked-workspace Rust caller scan finds no consumer outside the six modules,
their six facade test targets and their exact oracle/control scaffolding. All six
modules are removed together: `v3_ws_topics`, `v3_ws_commands`, `v3_ws_session`,
`v3_ws_state`, `v3_ws_public` and `v3_ws_private`. Their facade tests, orphan scripts,
generated gate reports and the exact unused book-level array registration are
removed too. Eighty manifest mappings retain their source identities with
not-applicable status; no identity fallback or alias permission is added.

The ticker composer and its remaining BBO/Trade declarations were subsequently
retired after their independent caller census. See `TICKER_RETIREMENT_REVIEW.md`.
The four remaining structural aliases retain their native contract runner; the
retired book-array identity still reaches the ordinary unsupported-alias refusal.

Validation uses the real `fix_audit_tests::` source/native registration probes,
plus the actual legacy public startup, private authentication and execution
reports, buffered orderbook snapshot/delta join and candle subscription tests.
Strict all-target Clippy checks the remaining compiler consumers. The full parity
index checks every source mapping, and the all-features cancellation inventory
checks the shared producers. This is dead-code retirement, not a claim of a
master-red behavior repair.
