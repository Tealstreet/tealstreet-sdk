# Bitunix regular and TP-SL snapshot raw-family admission

Reverified: a single malformed order row sets Read.invalid and holds every valid
peer. Scope this phase to REST order row admission; keep current WS generation
fences, position behavior, financial/side defaults and execution semantics.

Add request-local bounded rejection coverage keyed by native raw orderId/id,
using the existing string/integer/null text rules in a small identity decoder.
Known rejected raw IDs quarantine every duplicate row in either order. Unknown
identity holds absence only within the read's regular/standalone-TP-SL scope.
Overflow holds/conflicts conservatively. Emit sanitized coded ROW_SKIPPED.

Stage accepted raw parents, including valid rows mapping no visible children.
At complete-envelope commit, preserve protected cached raw groups together with
parent/attached brackets or both standalone legs and native routing metadata.
Accepted raw parents supersede their entire old group, so an unknown neighbor
cannot resurrect omitted brackets. Only admitted raw groups advance baselines;
keep the pending-execution, optimistic Creating/Canceling and terminal guards.

First readiness still requires both current regular/TP-SL halves. Any partial
initial half remains unready and retries; validate staged serials as well as
existing generation gates. Command-owned refreshes with any rejection cannot
prove a trading prerequisite, even if healthy UI peers can commit. Structural,
transport/native and stale snapshot failures stay atomic.

Regress known/unknown mixed rows, parent/attached and standalone sibling/native
retention, healthy peers/removals, accepted parent dropping omitted brackets,
numeric raw IDs/duplicate conflicts/overflow, initial half orders/recovery,
strict prerequisites, and unchanged execution watermark/fill/terminal cases.
Prove original failures; Bitunix/Binance native +strict checks, independent plan/
implementation reviews, full CI, actual preceding same-crate/shared integrations
and separate exact guard. Fresh actual673565c2e61 branch; no unlanded sibling code.

Verification: three original-source row-starvation regressions fail and all 13
regression families pass. 381 combined Bitunix/Binance native tests plus strict
Clippy/fmt/bans pass; existing optimistic/terminal/pending-execution/financial and
write-fence assertions remain unchanged. Independent plan/implementation reviews
CLEAR, including raw metadata/baseline cohesion and staged initial serial checks.
