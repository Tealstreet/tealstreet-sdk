# Source corpus provenance review

The seventeen native-floor guards were last pinned in `42c617ad0b`. Each oracle
hashes its maintained TS bodies, imports and supporting sources after evaluating
its cases. Their 15 distinct source inputs were compared byte-for-byte against
that checkpoint. Only `packages/safe-cex/src/types.ts` changed: it adds the
`binanceinverse` ExchangeName member, `hyrotraderMode` option and
`hyrotraderDemo` account field. These are type-only additions. The Lighter bodies,
imports, runtime enums, helpers, endpoints and severity definitions are unchanged.

Reviewed cohorts: account cache, batch placement, exchange error recovery,
historical fills, market reads, OHLCV, order actions, order mapping, order snapshot,
private recovery, public readiness, public subscriptions, reconciliation, runtime
initialization, single placement, stream watchdogs and trading preflight.

Only the expected content hashes change. The source hash computation, authored
case counts, native assertions, callback/error checks and mutation controls remain
intact. The account-cache target compiles and fails the original provenance guard
after its native assertions; the full venue workflow re-executes all seventeen
source/native cohorts with their refreshed hashes.

A complete scan of all 39 literal corpus guards found one additional stale
order-mapping pin, whose oracle uses `pin` instead of the original cohort's
`contentHash` key. Its 100 native cases passed before the old guard failed.
Its complete source graph
has the same type-only barrel delta and unchanged lodash version.

The signing capture has a separate source-map guard. Its reviewed `types.ts`
override is the eighteenth refreshed expectation, with the same type-only delta.
The other ten source hashes match. Its archived 92-case fixture stays immutable;
the test compares every signed payload, preimage, output and wire request before
the source-map assertion. The old override fails that assertion after all cases.
