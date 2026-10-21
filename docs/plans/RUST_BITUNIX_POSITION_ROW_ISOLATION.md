# Bitunix keyed position-row admission

Reverified global Read.invalid rejects every position peer when one typed row
fails. Scope this phase to positions only; order raw-family isolation/starvation
will be separate. Preserve financial mapping, side/identity defaults, serial and
write/preflight fences, REST/WS route precedence and all owner-gated policies.

Add request-local bounded rejection coverage keyed by native symbol text, using
exact PositionRow aliases in a small identity-only decoder. Known rejected
symbols quarantine all duplicate rows for that symbol in either input order;
unknown identity holds absence while valid peers may replace their own IDs.
Overflow holds/conflicts conservatively. Emit coded sanitized ROW_SKIPPED.

Stage admitted position IDs and route keys even for valid zero rows. After a
complete valid envelope, keep only rejected cached symbol groups, commit valid
peers and safely absent removals, preserve held route metadata and settings, and
never resurrect an explicitly closed zero row. Reject contradictory mode mixes
against held cached rows before any publication. Initial partial reads must not
claim readiness; command-owned after/prepared reads fail closed on any rejection.
Malformed envelopes, transport/native failures and stale reads stay atomic.

Regress identified bad A with healthy B/removal C, aliases and both hedge legs,
unknown+valid zero, duplicate invalid/valid order independence, mode coherence,
coverage overflow, held identity routing/settings, first partial readiness,
clean recovery and strict command prerequisites. Keep all existing account/
execution/write-fence tests unchanged. Red/green proof, independent reviews,
Bitunix/Binance native and strict checks, CI, actual intervening same-crate/shared
integration and separate exact guard. Source hold1637 remains active.

Verification: three corrected structurally invalid numeric-row regressions fail
original source and pass fixed source; preserve existing numeric-string fallback
instead of treating a legacy zero as a decoder error. Eight regression families
pass, with 376 combined native tests and strict Clippy/fmt/bans. Independent plan
and implementation reviews CLEAR. Source hold released after actual1637; real
integration and refreshed focused checks required before publication/merge.
