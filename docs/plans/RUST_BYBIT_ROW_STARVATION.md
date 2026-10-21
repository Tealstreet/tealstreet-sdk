# Bybit private row admission

Base: actual landing c63acfc19c2. Reproduce ordinary REST order/position and
structurally malformed WS order/position peer starvation. Admit healthy peers,
retain rejected cached keys and their protective lineage, and schedule bounded
repair with visible row diagnostics. Without safe identity, hold the affected
scope. Preserve serial/settings fences, optimistic/terminal state, complete
execution coverage and strict Hyro startup/account evidence. Regressions must
cover mixed known rejects, missing identity, later authoritative removal and
WS repair. Keep history marker ownership as a separate per-venue follow-up.

Run focused Bybit plus Binance native tests, strict Bybit Clippy, fmt/bans,
independent adversarial review and full CI. Integrate current landing when
required; run the exact scope guard separately before green merge.


Implementation receipt: bounded parent/symbol rejection coverage in the existing
Bybit joins, preserving terminal/freshness/optimistic state and independent product
scopes. Ordinary WS rows decode/map once and reuse typed admission checks; Hyro,
execution and wallet paths retain typed whole-frame handling. Unknown neighbors
cannot restore brackets omitted by an admitted parent, duplicate option Greeks,
erase valid closed-position Greek removal, or mix a new one-way ID with held hedge
legs. Bad WS identities protect their rows in older pending joins, coalescing
repair without globally invalidating healthy REST-only peers or dispatching writes.
Actual terminal/write/valid-position revision fences remain intact.

Eight new/strengthened regressions fail against original c63 source (no compile
failure), then pass with repair. Additional duplicate-key and mode-coherence
regressions protect the new admission boundary. Independent review caught the
Greek/follow-up interactions and hot-path duplicate decode; all corrected.

Review follow-up: repeated malformed WS order A and position A during two REST
walks both reproduced starvation with a global revision bump; a separate WS
unknown-neighbor/coherent-hedge case reproduced mixed mode exposure. All three
failed before the scoped-join/coherence correction and now pass. The ordinary
leverage-ACK race noted during design is preexisting and separate; this repair
does not claim a settings-revision fence absent from the current Bybit adapter.
