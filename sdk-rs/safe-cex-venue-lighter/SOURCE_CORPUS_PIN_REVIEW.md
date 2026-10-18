# Source corpus provenance review

The sixteen native-floor guards were last pinned in `42c617ad0b`. Each oracle
hashes its maintained TS bodies, imports and supporting sources after evaluating
its cases. Their 15 distinct source inputs were compared byte-for-byte against
that checkpoint. Only `packages/safe-cex/src/types.ts` changed: it adds the
`binanceinverse` ExchangeName member, `hyrotraderMode` option and
`hyrotraderDemo` account field. These are type-only additions. The Lighter bodies,
imports, runtime enums, helpers, endpoints and severity definitions are unchanged.

Reviewed cohorts: account cache, batch placement, exchange error recovery,
historical fills, market reads, OHLCV, order actions, order snapshot, private
recovery, public readiness, public subscriptions, reconciliation, runtime
initialization, single placement, stream watchdogs and trading preflight.

Only the expected content hashes change. The source hash computation, authored
case counts, native assertions, callback/error checks and mutation controls remain
intact. The account-cache target compiles and fails the original provenance guard
after its native assertions; the full venue workflow re-executes all sixteen
source/native cohorts with their refreshed hashes.
