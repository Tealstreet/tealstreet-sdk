# Bitunix explicit WebSocket route

Bitunix declares fallback-only candidates, so the shared resolver ignores an
explicit `tealstreetWsRoute: true` even though the TS venue uses the proxy on its
first attempt. Normalize only engine routing, selecting each declared Bitunix
fallback as its forced host when the option is true. Leave the shared resolver
and legacy TS consumers unchanged. Forced engine routes carry no fallback
candidate; unset/false keeps the two-silent-attempt direct-first policy.

Verify real engine transport targets for both public shard feed identities,
private and futures/business, first connect and reconnect. Compare each native
routing-plan host to the SDK target; extend the checked-in TS-produced native
routing matrix with true/false Bitunix plans and native selector checks. Keep
Bitget's fallback-only behavior unchanged. Keep direct transport's existing
unsupported-route diagnostic. Keep resolver golden snapshots unchanged; update audit and nearby docs. Prove first-connect regression against original code.

Run focused routing/transport tests, native routing selector and Bitunix/Binance
native gates, package typecheck/lint and strict Clippy/fmt/bans. Independent plan
and implementation review, green CI and separate exact guard before merge.
Integrate actual #1640 routing matrix/read-credential changes before final gates;
merge after the pending Bitunix source queue without unlanded sibling commits.

Plan review found that changing shared fallback-only resolution would regress
three TS socket callers that read only fallback. Revised to engine-only routing;
test unchanged shared resolver plus Bitget/direct controls explicitly.

Revised plan and implementation independently reviewed CLEAR. The explicit
first-host regression fails original engine routing; false/unset and Bitget
controls pass. Final 321 transport/resolver/matrix tests plus five focused
recovery/direct-provider controls pass; all prior 70 native matrix cases remain
structurally identical with two additions. Native matrix, 33 native routing and
378 Bitunix/Binance tests plus typecheck/lint/Clippy/fmt/bans pass.
