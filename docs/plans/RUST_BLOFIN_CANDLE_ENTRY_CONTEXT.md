# BloFin candle command-entry authority

Fresh branch feat/rust-blofin-candle-entry-context starts at actual1854/e2b03d5592a.
Use only landed1830 admission and existing1802 managed candle transport; no
unlanded history bridge or signer API. Recheck all open PR file lists before
source/tests. The held PublicTrades consumer is a separate branch with disjoint
lib.rs regions; its later draft1865 creates the reported whole-file collision.

Full venue inventory remains [RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md).
Current manifest has24 BloFin occurrences in20 hash rows across5 files. This
slice changes exactly lib.rs fingerprint
497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23 (count1):
the existing failed http_context candle-entry branch constructs Pending::Candles
and routes to the numeric legacy failure callback. Replace initial issuance with
cx.admit_http_command(Self::read_policy()); any Err returns immediately because
Rejected already emitted the genuine entry denial and Revoked grants no owner.
No numeric ID argument, fabricated Context or new fallback is allowed. Remove
the now-unreachable bare Pending::Candles numeric failure branch too; the counted
fingerprint is there, not at the initial issuance call. Candle pending values
remain inside Managed, whose Context already owns every physical callback and
terminal. Remove the now-unused numeric command field from Pending::Candles.
The remaining discovery fallback handles only Markets/Tickers; a bare candle
payload carries no completion authority.

All other23 occurrences stay unchanged: account7, closed1, history7 and trading8
retain their original map and signer/Plan/classifier prerequisites. No exemptions
or other-venue prunes. Expect24->23 only after the actual scanner proves removal.

Preserve the existing20-second Public/Read/Never policy, exact candle GET/query,
market/interval preflight errors, preparation/error hooks, native candle rows and
result sorting. Context rejection now uses the shared typed reason and
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
The stable venue phase is pinned to actual1854; actual1857 was deferred,
so fresh CI requires no new engine integration. No merge
until clean reviews, green CI and an overseer window with a fresh separate guard.

Caller/full-map reviews located the exact numeric branch and require its explicit
removal for the prune. All three independent plan reviews otherwise pass. The
updated source scope above is re-reviewed before production. New own draft1865
claims lib.rs for frozen PublicTrades callback hooks; report that whole-file
collision and await overseer authorization before the separate candle source
edit. Tests-first in unclaimed managed_public.rs can proceed independently.

## Executed entry proof

Overseer explicitly authorized overlap with our own frozen1865/lib.rs distinct
hunks: whichever lands second integrates the first. All three revised plan
reviews are CLEAN. Tests-first58e1d928002 executes two original-source assertion
RED failures (missing typed invalid-clock and capacity kinds) plus nine GREEN
compatibility controls; the production change makes all eleven GREEN without
changing the frozen test file. Capacity/reuse and preflight/success cleanup stay
at one clock and do not depend on elapsed expiry.

Source/adversarial, caller/documentation and full-map implementation reviews are
CLEAN; architecture No. Scanner proves23 occurrences in19 hash rows across four
files, removing only the lib497d count-one site. Full native and strict gates
follow the genuine current-landing integration before CI.

## Current landing and final local gates

Production9bd816d0ec4 integrates actual landingbe8889cb4b3 at7bd49c4d9fb,
including incoming Backpack/Kraken venue work. The only conflict was CALLERS;
resolution retains the full incoming prefix followed by our appendix once
verbatim. Independent preservation review is CLEAN: exactly eight intended
paths differ, every unowned incoming file and manifest prune is preserved, and
our complete BloFin/runtime dependency blobs match the pre-integration source.

Executed final local gates:494 BloFin/Binance native tests across43 targets,
strict scoped all-target/all-feature Clippy, scoped fmt, bans, request-context
checker and all13 checker regressions PASS. The native run uses those unchanged
runtime/dependency blobs; no incoming engine dependency changed. Independent
source/adversarial, caller/documentation, full-map and integration reviews are
CLEAN, architecture No. Fresh own-head CI and the separate current-landing guard
remain required before a granted parallel venue merge.
