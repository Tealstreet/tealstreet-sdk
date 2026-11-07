# Bitget private WS row admission

Start from actual landing 4f084411cf7. Reproduce structurally malformed WS order
and position rows discarding healthy peers for Classic and UTA. Decode ordinary
order/position rows independently, retain rejected cached identities and parent
protection, and keep WS snapshot omission coverage, freshness clocks, native
metadata and settings fences correct. Unknown identity cannot prove absence;
valid peers can advance. Coalesce read-only repair using existing bounded timers.
Do not relax the financial fill decoder or fabricate execution identity/amounts.

Plan/source audit and independent review must cover snapshot versus updates,
missing identity, duplicates, terminal rows, pending reads and active repair,
product filters, authoritative later empty snapshots and no financial fabrication.
Run touched Bitget plus Binance native tests, strict Bitget Clippy/fmt/bans; push
full CI/replays/size. Merge after Bybit row/fence work under fresh separate guard.

Phase receipt: raw ordinary WS arrays admit healthy Classic/UTA peers; cached
position scope and per-ID order revisions protect rejected identities. Four
initial regressions proved red. Review additionally reproduced repeated malformed
snapshots starving fresh REST-only peers: rejected observation barriers are now
separate from full financial snapshot authority/freshness. That two-walk test is
red/green, with native/strict gates passing. Partial malformed/foreign payloads do
not claim useful authenticated financial traffic. Metadata route-fence regression
and final independent review remain before opening/marking this batch ready.

Final repair receipt: preserve financial WS timestamp/update independently from
rejected observation arrival; malformed data cannot renew or erase real financial
freshness. A prior clean full snapshot remains an older-read fence after partial
traffic without blocking fresh REST-only peers. Held rejected positions never
republish stale settings. Global mode admission covers partial/mixed snapshots,
clean deltas and REST joins; complete coherent proof still permits mode changes.
All-rejected initial traffic cannot manufacture loaded-empty readiness. Native
plan and trailing metadata share the existing revision/opened/terminal fence, so
retained protection cannot acquire a stale cancel route.

Twelve regression families proved red across initial and adversarial corrections.
492 focused Bitget/Binance native tests plus strict Clippy/fmt/bans pass; two
final independent reviews clear. Financial fill-channel admission and execution
accounting remain unchanged. Source merge holds for entropy PR #1624 after
#1617/#1621; integrate its actual merge and rerun CI before landing this PR.
