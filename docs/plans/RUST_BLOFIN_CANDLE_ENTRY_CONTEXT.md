# BloFin candle command-entry authority

Fresh branch feat/rust-blofin-candle-entry-context starts at actual1854/e2b03d5592a.
Use only landed1830 admission and existing1802 managed candle transport; no
unlanded history bridge or signer API. Recheck all open PR file lists before
source/tests. The held PublicTrades consumer is a separate branch with disjoint
lib.rs regions; no other open PR currently claims this venue source.

Full venue inventory remains [RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md).
Current manifest has24 BloFin occurrences in20 hash rows across5 files. This
slice changes exactly lib.rs fingerprint
497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23 (count1):
the existing failed http_context candle-entry branch constructs Pending::Candles
and routes to the numeric legacy failure callback. Replace initial issuance with
cx.admit_http_command(Self::read_policy()); any Err returns immediately because
Rejected already emitted the genuine entry denial and Revoked grants no owner.
No numeric ID argument, fabricated Context or new fallback is allowed.

All other23 occurrences stay unchanged: account7, closed1, history7 and trading8
retain their original map and signer/Plan/classifier prerequisites. No exemptions
or other-venue prunes. Expect24->23 only after the actual scanner proves removal.

Preserve the existing20-second Public/Read/Never policy, exact candle GET/query,
market/interval preflight errors, preparation/error hooks, native candle rows and
result sorting/filtering. Context rejection now uses the shared typed reason and
read outcome convention. Duplicate command entry must neither allocate another
HTTP preparation nor retire the original owner. No per-venue retry, classification,
clock, admission or finalization policy is introduced; architecture answer No.

Commit this plan, obtain independent source/adversarial, caller and full-map
reviews, then commit assertion-RED tests separately. Controls: duplicate initial
command during parked preparation preserves the first owner's terminal; genuine
registry capacity refusal emits exactly one shared typed failure with no opaque
I/O and succeeds again after retiring unrelated background roots; invalid-clock
entry refusal is typed and permits same-ID reuse with a valid clock; fixed-clock
270 repeated market/interval preflight failures and successful results do not
exhaust owners. Use genuine engine callbacks and existing prepared_actions, no
synthetic authority or replacement timers. Existing native candle mapping and
expiry controls run unchanged.

Implement only the initial candle admission seam, prune the one proven own entry,
append actual caller/audit/quality behavior and execute BloFin+Binance tests, scoped
strict Clippy/fmt, bans and both request-context checks. Re-review source/caller/map.
Integrate actual1857 once before fresh CI in the stable venue phase. No merge
until clean reviews, green CI and an overseer window with a fresh separate guard.
