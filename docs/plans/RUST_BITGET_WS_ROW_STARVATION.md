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
