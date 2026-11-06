# Bitget protective edit live fixes

HTTP 429, native 429 and UTA 25000/25001 write answers are Unknown. They cannot
start cancel/place fallback or a put-back. The failure tells the caller to check
Open Orders. A released transport failure remains Unknown as before.

Classic TP/SL native modification supplies the retained trigger on amount-only
edits and keeps a known native execution price and trigger source. Source frames
carry execution prices in Bitget VENUE_STATE version 2. Version 1 remains readable
with no execution-price evidence, preserving account detection and other writes.
A modify without retained execution evidence refuses NotSent and asks for a
refresh; it never guesses zero (market). Unknown or malformed versions are refused.

UTA TP/SL put-back uses the original venue quantity, independent of the current
position. A full-position order keeps its zero/full sentinel through a price edit
and any put-back. This does not establish that UTA's native strategy-modify API
accepts full-mode edits without qty; its partial-only qty contract remains open.

Classic moving_plan uses the TPSL modify contract and retained rangeRate;
track_plan keeps its trigger-plan modify contract. Account nuke includes both.
The separate protective-guard change supplies the final cancel veto, native
attached editing and typed put-back presentation.
