# Bitunix explicit WebSocket route

Bitunix declares fallback-only candidates, so the shared resolver ignores an
explicit `tealstreetWsRoute: true` even though the TS venue uses the proxy on its
first attempt. Add an opt-in property to fallback-only strategies and enable it
for Bitunix public/private/business routes only. Resolve the explicit option to
the declared proxy host, without direct fallback candidates; unset/false keeps
the existing two-silent-attempt direct-first policy and watchdog recovery.

Verify real engine transport targets for both public shard feed identities,
private and futures/business, first connect and reconnect. Compare each native
routing-plan host to the SDK target; extend the checked-in TS-produced native
routing matrix with true/false Bitunix plans and native selector checks. Keep
Bitget's fallback-only behavior unchanged. Keep direct transport's existing
unsupported-route diagnostic. Update only affected resolver golden snapshots,
audit and nearby docs. Prove first-connect regression against original code.

Run focused routing/transport tests, native routing selector and Bitunix/Binance
native gates, package typecheck/lint and strict Clippy/fmt/bans. Independent plan
and implementation review, green CI and separate exact guard before merge.
Integrate actual #1640 routing matrix/read-credential changes before final gates;
merge after the pending Bitunix source queue without unlanded sibling commits.
