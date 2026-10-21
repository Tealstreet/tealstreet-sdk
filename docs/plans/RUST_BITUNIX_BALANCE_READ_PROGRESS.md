# Bitunix balance progress during private notifications

Fresh landing branch at `67e81cfc13b87235003a4178a784664d399cbbe2`.
#1658 fixes ordinary Orders/TP-SL WS generation starvation; #1641 fixes
positions. Balance notifications still advance generation and reset debounce,
so otherwise valid balance REST responses can be discarded indefinitely.
Repeated balance issuers can also supersede a slow read before it completes.

1. Reproduce continuous-notification balance starvation, slow-read serial
   replacement and postponed first repair using native engine regressions.
   Preserve explicit balance/free/uPnL assertions and settings-ACK controls.
2. In Bitunix account state/request metadata, separate acknowledged position-mode
   revision from notification generation. Admit only the current ordinary balance
   serial and mode-write revision. Keep command-owned admission strict and never
   reuse a stale mode read after an accepted mode ACK.
3. Coalesce ordinary balance issuers while a read is younger than the existing
   ten-second lost-read bound. Record issue time at actual dispatch, retain the
   first notification repair deadline, and keep matching-only completion/failure
   semantics. Preserve the existing500ms polling policy and all financial mapping.
4. Test lost-read replacement, late old success/failure, refused mode writes,
   valid current mode repair, malformed/failed balance responses and reconnects.
   Migrate any old immediate-overlap fixtures to real captured deadlines while
   preserving their financial assertions. No retries of writes or guessed data.
5. Independent correctness/adversarial reviews; Bitunix+Binance native tests,
   strict Clippy/format/bans with four Cargo jobs. Push a draft for full CI;
   do not merge during #1661's source window or this local/CI-only task.

Account recovery #1662 is held separately and is not carried into this fresh
branch. It touches the same read/currentness paths: integrate actual landing
in the agreed later order and rerun CI before any eventual merge. Respect all
owner/live-capture exclusions from RUST_PORT_WIRE_SWEEP; no new budgets,
local release WASM builds, replay matrices, timing gates or load tests.

Plan audit: clear with one required release fix. Synchronous signing failure must
mark only its balance serial answered before scheduling ordinary repair; no HTTP
was sent, so coalescing must not fabricate a ten-second in-flight wait. Existing
position-mode ACK financial regressions remain mandatory. Add refused, malformed
and transport-lost mode ACK controls that never advance the balance write fence.
When #1662 later integrates, retain its pause gate before any issue timestamp or
serial mutation and use the balance serial/write revision for auth currentness.
