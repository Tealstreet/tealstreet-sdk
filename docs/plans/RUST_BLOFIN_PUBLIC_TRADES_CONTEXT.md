# BloFin PublicTrades Context adoption

Base: actual #1802 and #1846 are landed at 74f3e9d3983cf53f86235cd80fe5bc9404943e6a. Fresh branch feat/rust-blofin-public-trades-context starts there. Authorized exact shared bridge stack is #1848 reviewed703731e6d546c02944d631e2552a0d2086215b89, genuinely including actual1830/0001c78eeb2. Re-integrate actual seams before final consumer proof/merge. Never edit engine/seam files. Recheck every open PR file list before source/tests; report collisions.

Native scope is only Query::PublicTrades. No private history, closed orders, signed requests, Plan, group completion, credential selection or classifier implementation. Full baseline inventory remains RUST_BLOFIN_REQUEST_CONTEXT.md; this slice may leave a shared legacy physical dispatch site while private consumers remain. Prune only scanner-proved removed BloFin entries, never claim zero by delegation.

Use genuine cx.admit_http_command(Blofin::read_policy()) at native PublicTrades command entry, before preflight or I/O. The existing read policy is Public, Read, 20000ms, Never and no credential. Rejected already owns terminal denial; Revoked grants nothing; neither branch uses numeric fallback. Construct ReadCompletion from the exact original root and command. A distinct managed PublicTrades pending variant avoids moving legacy private history authority or adding a local authority-dispatch wrapper.

Use shared ReadCursorHistory and its borrowed admit(cx, exact physical leg) guard for freshness, staged rows and accepted page advancement. Keep immutable Cx borrow scoped away from diagnostics and terminals; re-admit as required. Context refusal returns without admission/publication. Context-only finish/reject/finish_with Results are handled without unwrap or numeric fallback. No local finalizer implementation; reuse existing encode_result query shaping through shared finish_with. Stream failure and malformed/absent envelope must not publish staged prefixes or history.incomplete. Cleanup after physical expiry must use the shared typed terminal without a timed live validator.

Preserve exact native route, query and bytes: unsigned GET /api/v1/market/trades, instId first and limit second; limit default100, clamp1..100; empty headers even for keyed accounts; streamed data envelope; unknown market preflight InvalidInput/NotSent; one page always, no cursor continuation or automatic retry. Preserve row instId/time/id filtering, ts then createTime fallback, unknown-market identity-consumption order, duplicate trade IDs, native row order, market contract-size quantity cleaning, quoteQty multiplication and side conversion. Existing Row codec and encode_result are shared native functions; do not copy cross-cutting authority policy into venue helpers.

Committable sequence: commit inventory/plan and caller contract; independent plan/source/caller/full-map reviews; separate tests-first commit with assertion-RED on current actual source; production move; exact shrink-only manifest prune; CALLERS/AUDITS/QUALITY appendix with actual behavior; independent source/adversarial, caller and map reviews; focused native gates plus fresh CI. Record exact source/test pins and real venue receipt to cxxyqr, independent of shared bridge engine fixtures. Architecture answer must remain No: validation/completion authority stays in engine.

Controls: (1) genuine Prepare/Bind/Claim precedes raw HTTP and exact public request bytes remain unchanged; (2) response before release or claim cannot admit rows/complete; (3) response at deadline cannot admit a streamed prefix, typed physical expiry settles once; (4) transport/malformed/absent data/native refusal first-page failures yield typed failure with no rows/marker; (5) duplicate response and stale prior-generation delivery cannot affect same numeric ID reuse; (6) unknown-market preflight settles once without HTTP and fixed-clock repetition beyond registry bound proves owner retirement; (7) nonempty/empty success, limit bounds, ts/createTime, duplicate IDs, contract-size/base+quote quantities, row order and keyed/keyless public header equivalence remain compatible. Add native interposed command admission and exact terminal timeline assertions when needed to exclude silent owner leaks.

Verification: all BloFin plus Binance native tests, strict scoped all-target/all-feature Clippy, fmt, bans, current request-context checker and13 checker regressions. Reuse existing shared prepared_actions/prepareRecordedHttp fixture plumbing; run focused TS checks if its native-to-WASM fixture changes. CI owns WASM release/replays/performance. Fresh current-landing trial and separate revert guard are required; no merge until owner grants a window, own integration CI green, source reviews clean and shared prerequisites actual.

Caller review additions: PublicTrades accepts rows for any known market even when it differs from requested symbol; never add requested-symbol filtering. Unknown-market rows consume fresh trade identity before being skipped, so a later known row with that identity remains suppressed. The outgoing limit does not truncate result rows: preserve oversized native responses and unsorted timestamp order. Timestamps stay milliseconds. Only exact lowercase sell maps to Sell; other values retain existing conversion. Missing/default numeric fields and nonnumeric trade IDs must not gain validation. Assert every serialized Trade field, including quoteQty, in native compatibility controls.

Full-map review: baseline24 BloFin occurrences, seven in history across four hash rows. PublicTrades-only separation need not remove a ratchet entry: the one physical legacy dispatch remains for private history, and conservative request.query macro matches may remain. Expect24→24 unless the scanner proves a real own-entry removal; document the unchanged count honestly. No instType/SWAP filter may be added for PublicTrades. Whole-file collision review must include lib.rs, history.rs, existing test support and any shared fixture used.

## Current inventory and area mapping

The full native-expression and caller map is retained in [RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md); its original28 baseline is historical. Current actual inventory is24 occurrences in20 hash rows across5 files. The table below names every current row. No other venue entries may change. The PublicTrades slice binds a distinct shared ReadCursorHistory owner and managed delivery while the shared private-history dispatcher can remain live; no reduction is promised solely by splitting that path.

| Current path | Exact fingerprint | Count | Destination / remaining owner |
| --- | --- | ---: | --- |
| `venues/blofin/src/account.rs` | `1a8e09311f93f0d7d9c69519e6d325d16f9366b33946b861ab28ffaa45d927f5` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `37bc238a40045dda234a3daaf68329976cef33caee8f13734a73f526bfef32cd` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `4f2afd7cb212a9914e8dece3dbb3733c167148c003035b2f7628bb8aab8876c8` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/account.rs` | `e2c8da59c7ed525a6d5342feecd9df9ef825904a02af1c97de61f39d1a7abf89` | 1 | Held private/background/account path: shared signer plus Read/Session lifecycle; paired work needs shared Plan and typed publication. |
| `venues/blofin/src/closed.rs` | `bb5ed84ac1882574929599a10507277895dbe7521e20ef78b3f641d2ff116b38` | 1 | Held signed closed-history flow: shared signer, ReadCursorHistory and Plan where native preparation/paging requires it. |
| `venues/blofin/src/history.rs` | `4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7` | 1 | Conservative query macro; preserve native private decoding and retain until scanner proves removal. No authority or I/O by this match. |
| `venues/blofin/src/history.rs` | `60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa` | 3 | Conservative query macro; preserve native private decoding and retain until scanner proves removal. No authority or I/O by this match. |
| `venues/blofin/src/history.rs` | `d7f5d65d34ca3d770c22743829f9f73e57682c52d8b63bb54030b167abb8fa43` | 1 | PublicTrades: genuine Read root, managed streamed delivery, guarded ReadCursorHistory, shared finish_with. Private paged/signed consumers remain for SignStep/Plan adoption; conservative query macros remain tracked. |
| `venues/blofin/src/history.rs` | `f33c5fe4590c54205bbed83f2a6cf21ba3ccfd0cbdab18d6832e43ae0c4d36a0` | 2 | Conservative query macro; preserve native private decoding and retain until scanner proves removal. No authority or I/O by this match. |
| `venues/blofin/src/lib.rs` | `497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23` | 1 | Existing candle pre-Context refusal; separate follow-up adopts actual command-entry rejection seam. No new fallback may be added. |
| `venues/blofin/src/trading.rs` | `0dfe0bf78ed209b6c5b74f9867340478f38851c8e4151adc3396745bf7db467b` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `349d1b39839519801db71f4348af20496c0443cb14880093d11ecbbe7cc68843` | 2 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `368af85f963a94d4984aa5e5d920f2728ce55064b20be7dea5116c8cde1a251e` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `3c6e80032656d492647546e807c37c73b3593d225419189ac1d8531e2e16fb1b` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `604f28d91adfdb4cd0caac06047abd9569f9a6c248b698a784032256d7cf8047` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `af0b7f916c07c8f53f3b2731f1c70b01fa5b0253ae68ebc878782fdf2ef51817` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |
| `venues/blofin/src/trading.rs` | `afb3c3744f13da9606f5f456bd95797992a932cf227eb74b2fe4405e3c64ee98` | 1 | Held trading flow: shared signer/Plan/CommandWrites and frozen classifier table; preserve accepted IDs and native per-leg outcomes. |

## Ownership and readiness

Open-PR file lists were checked after both callback owners landed; BloFin lib.rs, history.rs and the proposed managed_public_trades.rs are unclaimed. Shared engine/history files remain owned by #1848 and are consumed only through the exact reviewed703731e6d54 merge at5e704d7a1cc. Its engine tree is byte-identical to that reviewed head; every incoming74f venue, package, host, harness and inventory byte is preserved. Only CALLERS required a conflict resolution, full incoming text followed by the bridge appendix once verbatim. No bridge file will be edited by this consumer. Actual bridge integration is required before final merge.

Caller and full-map plan audits are CLEAN, with the compatibility and honest24→24 count refinements above. A final independent source/adversarial plan audit precedes tests/production. No native consumer receipt or production readiness is claimed by this plan commit.

## Typed failure plumbing required by source review

Exact703 engine supplies typed released HTTP failures through Venue::on_http_failed, including Timeout at physical expiry and Internal for undecodable whole response bytes. New PublicTrades pending must retain that VenueError through the shared history terminal. Its preparation failures likewise retain the typed VenueError from on_sign_error/on_http_preparation_failed. Existing private/non-PublicTrades callbacks keep their current behavior. Do not route these new failures through the old message-only fallback or add text, status or deadline classification in the venue. Immediate request_managed refusal uses the shared Error::venue_error conversion. Parked preparation expiry and released physical expiry need separate executed controls.

The source/adversarial plan review is CLEAN after this clarification: no missing bridge or error primitive, no engine edit, no local finalizer. The bridge owner independently reviewed the committed plan as API-compatible. All three required plan reviews are now satisfied before tests-first.

Current reviewed bridge pin: ee8cdb6e79b15263b7d8b829ed0dd4cef3529b79, announced by the overseer after actual1820 integration. The earlier703 bridge remains an ancestor with unchanged bridge API/source/tests. The composed engine/source/inventory now matches ee8 exactly; only this consumer plan and its README row differ. Every previous CALLERS paragraph remains in the incoming version. Original RED assertions remain unchanged.

## Executed native consumer proof

Tests-first c158897781f and the fixture correction8eb3aced08b ran against
unchanged production, including genuine actual1837 at e2d0a048f36: six assertion
RED controls and five compatibility GREEN controls. Production faa39dde455 makes
all eleven controls GREEN. The frozen dedicated test file is unchanged by the
production commit. Existing history test helpers now use shared prepared_actions
at the exact query timestamp; their native payloads and quantities stay pinned.
The existing mixed private/public failure matrix now expects Network solely for
released PublicTrades transport failure; private Internal expectations remain.

Actual1854/e2b03d5592a is genuinely integrated at3dbd753e95e. Independent
preservation review is CLEAN: all four bridge blobs exactly match reviewed ee8,
the registry delta is the same original fifteen-line bridge delta, the complete
BloFin crate matches faa39dde455, and every incoming unowned file and inventory
entry is preserved. No bridge/core source was edited by this consumer.

Executed on this actual1854 integration: 501 BloFin/Binance native tests across
44 targets; strict scoped all-target/all-feature Clippy; scoped formatting; bans;
request-context checker and all13 checker regressions PASS. Source/adversarial,
caller, documentation and full-map reviews are CLEAN; architecture answer No.
The manifest remains unchanged at24 BloFin occurrences across20 hash rows.
No actual1848 landing, final integration CI, merge readiness or zero-legacy claim
is implied by this native consumer receipt. The stable venue phase is pinned to actual1854; fresh CI uses that
integration. Actual1857 was deferred by the overseer until after this phase.

## Actual bridge integration and capacity control

Actual #1848 (`6872554ac85244ec7b768b79af7187013dba429a`) and current landing
`c473342d48363a057c98c59292abb957462f3590` are genuinely integrated, most recently
at `c0d5952a65331c99f107b9a80d41aa88e2760da9`. All six shared history/authority
blobs are byte-exact to actual landing; this consumer edits no engine source.
The five consumer source/test files match the bridge owner's isolated audit
`cbfcba64f537`, except the capacity assertion now derives Error::Capacity's
shared display text. No synthetic audit commit was merged or claimed as this
branch's executed receipt. Incoming candle command-entry adoption is preserved.

The old queue-refusal control fails on the actual engine only because it expects
`Too many requests in flight` instead of `Engine request capacity reached`.
Test-only commit `6212289e4e9` changes that one expected message to
`Error::Capacity.to_string()`; its kind, NotSent outcome, no-I/O and owner-retirement
assertions remain intact. The original six semantic REDs/five compatibility
controls and native production logic remain unchanged.

The current inventory is **23 occurrences /19 rows /four files**, unchanged by
this PublicTrades slice. The old 24/20/five snapshot above predates separately
landed #1867's candle-entry prune. Current manifest equals incoming exactly;
shared private dispatch and conservative history macros remain tracked.

The current focused receipt runs 32 native BloFin controls (history 12, owned 9,
managed 11) plus three engine bridge controls. Scoped strict Clippy/fmt and both
request-context checks (15 regressions) pass. These are scoped native receipts,
not a fresh whole-package or performance proof. Fresh normal integration CI and
a new venue window/guard are required before merge.
