# Bitunix account read recovery

Remaining finding: rejected private account reads warn and retry roughly every
second, without selected-credential auth receipts or bounded failure backoff.
Use existing engine auth/read-retry APIs; preserve financial admission and writes.

Add typed read-only response classification and sanitized coded diagnostics for
balance, positions, regular/TP-SL orders and per-symbol settings. Only strict native
403/10003/10004 proof on the documented successful HTTP envelope may report a
credential rejection. Signature10007, nonce10010, timestamp10011, bare HTTP401/403,
proxy HTML, transport and malformed envelopes never prove an invalid key. Preserve
native/HTTP/rate metadata without copying raw bodies into diagnostics. Existing
write/history classifiers and retry behavior are outside this change.

Implement account_auth_outcome using the signer-slot/scope tags already captured
by #1640. Accept only a current request with native success and its required
complete list/object. Empty valid lists are credential acceptance; row mapping
failure is not key refusal. Merge regular/TP-SL auth evidence so a successful
sibling cannot heal a refused endpoint. Settings must likewise retain refusal
per symbol; do not map arbitrary symbols into the shared 32-bit endpoint bitmap.
Stale serial/write/generation callbacks cannot heal current evidence or retry state.

Use startup::Backoff per read source, with a venue-wide rate pause. Gate every
issuer: initial startup, tick, debounce, lost-read timers, settings interests and
command preflight. Keep one scheduled retry per key. Fail an owned prerequisite
once with NotSent while scheduling ordinary background repair; never reissue its
command or any POST. Other failures back off from1s to30s with existing jitter;
HTTP418/429 and native10005/10006 respect at least60s and longer bounded Retry-After.
Only the matching validated response resets a source. A sibling success cannot
clear a still-active host rate pause. Keep endpoint token/deadline ownership intact.

Regress explicit refusal versus ambiguous evidence, primary/secondary slots,
accepted empty lists and malformed rows/envelopes, merged order/settings scopes,
late old callbacks, all issuer paths during ordinary/rate pause, capped recovery,
synchronous/stream/transport failures, and owned no-write behavior. Prove original
failures, run Bitunix/Binance and strict gates, independent reviews, full CI and a
separate exact guard. Consult the official error table:
https://www.bitunix.com/api-docs/futures/ErrorCode/error_code.html

Fresh branch from actual landing; integrate exact reviewed #1658 as a genuine
read-admission dependency. No merge while any predecessor remains unlanded.
Owner/live-capture decisions and signature resend remain excluded.

Plan-audit refinements: validate the complete required shape before mutating auth
groups, use a per-symbol settings attempt serial, and aggregate refused settings
symbols without hashing them into fixed endpoint bits. Keep one clean deferred
read scheduler for healthy keys blocked by a host pause; do not count poll attempts
as failures. Apply the60s floor even to a short Retry-After header. Retained partial
rows or superseded snapshots do not constitute rejected credentials or a failed
transport read. Preserve status/headers on malformed private streams and sanitize
private diagnostics. Independent plan audit is clear with these constraints.
