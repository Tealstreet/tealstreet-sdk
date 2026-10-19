# Phemex perpetuals Rust port

Branch: `feat/rust-phemex`, based on `origin/feat/rust-rewrite`.

## Plan

1. Public: typed streaming products/tickers, packed ticker updates, sequence-aware snapshot/delta books, candles, connection watchdog. Review and commit.
2. Private: exact HMAC signing, authenticated aop_p stream, independent USDT/USDC account slices, per-market mode/settings even when flat, stale snapshot guards, execution deduplication. `Triggered` maps Closed but is never terminal. Review and commit.
3. Trading: g-API PUT/DELETE sequenced jobs, preflight whole command and caller IDs, bracket/trailing formatting, cancel/amend/settings with acknowledgement guards and no write retries. Review and commit.
4. Queries: bounded typed paged reads and explicit unsupported contracts where venue/TS lacks a safe route; candle reads. Review and commit.
5. Harness/gates: wasm/SDK registration, strict signed-request and WS audit, documented private tape and recorded public fixture, known TS fixes, Binance regressions and all CI checks. Review and commit.
6. Landing: merge latest landing branch, rerun gates, push and PR into feat/rust-rewrite, post adversarial reviews, fix until clean, merge only green CI, cut fresh follow-up branch.

## Plan audit

USDT/USDC PerpetualV2 g-API uses Rp/Rv values. Legacy Ep/Er scaling is handled only where the TS wire contract needs it. Order snapshot is WS-owned; REST position polling must not erase orders. Position mode must survive zero-size account rows. Failed currency reads retain their own prior slice. Every socket requires server.ping and bounded reconnect/watchdog. No credentials or live private writes are needed for gates.

## Progress

- Read current landing brief/spec and Phemex runtime guidance. Full-auto user authorization overrides the plan skill confirmation pause.

- Public initial native tests pass (4), typed market/ticker/candle/book implementation and wasm/SDK registration prepared. Root plan audit added malformed-row rejection, bounded book snapshot deadlines, native-symbol preservation and generation fences.
- Private/trading/query implementations prepared in disjoint files with independent tests and cross-review in progress. Query policy uses documented textual `/api-data/g-futures/{trades,orders}` schemas; generic income remains an explicit unsupported result pending verified ledger semantics, matching the absence of a safe TS capability.
- Public live recording exposed a real engine limitation: `/public/products` is 2.54MB with large unrelated arrays beside `perpProductsV2`, exceeding the 1MiB metadata cap. A bounded opt-in sibling-list omission mode is being implemented and independently reviewed; the memory limit will not be raised.
- Harness now separates authenticated/private and public sockets sharing a URL, validates exact REST/WS signatures and subscription order, and explicitly bootstraps only flagged initial auth/snapshot records. Harness tests 17/17 pass.

### Phase reviews and first gates

- Public reviewed independently by private_impl; fixed strict two-sided REST depth validation, positive product precision, monotonic 429 backoff, native candle symbols. Added bounded `PathOmitLists` streaming for the 2.54MB catalog; engine 24 tests pass and root reviewed parser changes.
- Private reviewed by trading_impl; fixed mutation of caches on malformed snapshots, permanently blocked amendments after fills, and stale currency balances. Account 14 and signing 2 tests pass. Triggered→Closed remains explicitly nonterminal.
- Trading reviewed by public_review; fixed unready/disconnected cancel-all admission and settings ACK/context races. 23 tests pass, including real-value PUT routing, caller-ID guards, Init acknowledgements, partial cancel/fill races, brackets and exact split sizes.
- Queries reviewed by trading_impl; fixed historical close-on-trigger reduce-only semantics, zero-sized close-entire stops, and trailing metadata. 12 tests pass. Generic income is explicitly unsupported pending verified ledger scope/sign semantics.
- Live full-depth public fixture: 20 seconds, 533 replayed frames, 891 markets, 324 tickers, 2 books. First passing median gate: TS/Rust CPU 277.9/90.9ms, steady 178.4/30.6ms, retained 5.8/2.8MB. Further final gates follow landing merge.
- TS bugs fixed and recorded in KNOWN_TS_FIXES: implicit depth30 subscription leaves deep levels stale; lost outer sequence discards buffered deltas; periodic full snapshots must replace old levels; zero-open percentage must remain finite; fills deduplicate by execution rather than order/message; cumulative executed exposure survives cancellation; exact split lots and partial-fill replacement remain bounded and fail closed.
- Crypto audit: this port reuses only existing HMAC/SHA/UUID primitives and replaces no currently called crypto helper; no additional provably unused helper introduced.

### Landing integration checkpoint

- Merged BingX landing head `9ddec5d0a4`. Independent merge reviews preserve incoming selectors, elapsed memory quiescence, strict write/subscription gates, and all Phemex hooks.
- Engine 36 tests pass after combining catalog omission and incoming selector aliases. WASM registers all seven venues. The Phemex public gate passed before integration; final full matrix will run on the combined artifact.
- Private first run found TS drops flat ETHUSDC leverage settings; the independent taped-row oracle now restores that known setting, cumulative fills, and per-execution fills without referencing Rust output.
- Harness review fixed unsigned history reads accepted by audit, missing full-depth subscription validation, and ambiguous shared-endpoint close records (fail closed). Added independent oracle regression and signature/depth tests.

- Combined seven-venue artifact after BingX integration: 2,923,610 bytes. Aggregate static budget becomes 3MiB (3,145,728 bytes, 7.6% headroom); CPU/memory comparison remains strictly Rust<TS. Final Phemex gate medians so far: 219.0/73.0ms CPU, 141.7/20.5ms steady CPU, 5.7/2.8MB retained; private 14 frames PASS.
- Full source workspace fmt/clippy/tests and wasm clippy in progress; root typecheck 44 tasks, lint 28 tasks and Phemex TS 39 tests pass. Source Rust suites: public6/private14/signing2/trading23/queries12 =57 tests.

### Final local validation

- Phase5 adversarial audit is clean at `78e6666862`; shared-URL close records now fail before ambiguous socket selection. Depth and authenticated-history negative controls pass; Phemex public-dispatch and full-depth/snapshot TS regressions pass.
- Workspace bans/fmt/clippy/native tests and wasm32 clippy PASS. Phemex57 native tests PASS; engine36 tests PASS. Full SDK6,497 tests /454files PASS; Phemex39 TS tests PASS; wasm19, host10, harness34 PASS. Root typecheck44 tasks and lint28 tasks PASS.
- Seven public gates PASS on the combined artifact (TS/Rust CPUms, retainedMB): Binance182.9/73.8,4.2/2.4; Bybit288.6/193.1,6.2/2.6; BingX111.5/72.9,3.6/2.1; Crypto.com65.3/58.2,1.9/1.6; OKX748.4/556.0,5.1/3.5; WEEX112.6/71.0,5.1/2.3; Phemex221.6/73.3,5.7/2.8. Phemex steady141.8/19.8ms. All ten private replays PASS: Binance, Bybit, BingX, Crypto.com, WEEX, Phemex and four OKX variants. Total17/17 gates PASS; no skipped gates.

- Landing publication checkpoint: final source head `78e6666862` passed all17 local gates and required checks. PR into feat/rust-rewrite next, followed by adversarial reviews posted as comments; merge waits for green CI.

### PR1310 adversarial review, round1

- Posted independent reviewer findings on PR1310. Two public P2s fixed: required full WS snapshot sides and startup admission during active429 pause; both focused native controls pass. Shared oracle now follows latest exact row time for flat leverage/margin changes; stale row/new envelope regression passes. Harness35 tests pass. Phemex public/private gates rerun PASS after public fixes.
- CI secret scan identified only the synthetic HMAC signature in original tests/sign.rs:52, independently generated from key `key`/secret `secret`; exact historical fingerprint added to .gitleaksignore and local history scan PASS. No broad exclusion.
- Three private defects are being addressed before another review: envelope time promoting stale rows, snapshot absence fencing, and semantic-invalid snapshots advancing sequence/loaded state. Merge remains gated on final green CI.
- Round1 private/trading findings fixed and independently re-reviewed: native row versions take precedence over envelope/client clocks; REST absence fences only observed versions; semantic-invalid frames do not advance state; standalone amendments preserve trigger provenance; unconfirmed cancellation cannot release replacements; imported conditional limits fail closed before writes; margin-only changes preserve asymmetric side leverage; exact decimal lot maxima remain valid. Account21/signing2/trading34 tests and all-target clippy PASS. Balance replay and 500ms client-clock-skew controls also pass independent review.
- Next integration phase: merge MEXC landing `0018bba71a`, preserve incoming virtual replay clock/socket ordinals and release size optimization alongside Phemex bootstrap, exact request matching and independent oracles; rerun all eight public and eleven private gates before final CI.
- MEXC integration resolutions reviewed: engine/store/wasm registration clean; PathOmitLists production retained with incoming parser tests; both venue oracle hooks retained. Replay combines incoming virtual clock/ordinal routing with explicit Phemex bootstrap, shared-endpoint fallback, original strict request/response matcher and real-time readiness deadlines. Incoming size-optimized Wasm build retained. Workspace native tests/clippy, wasm clippy, bans/fmt, typecheck44 and lint28 PASS; full replay/SDK validation follows on combined source.
- Expanded-matrix integration finding: Crypto.com readiness hook ran before its market seed after removal of the old first-socket wait. Restored the ordinal-aware first-socket fence; two production-play regressions pass. Independently reviewed alongside account comparison's recursive object-key canonicalization (arrays and wire types remain exact). Corrected Crypto.com public and Phemex private gates PASS. All49 harness tests PASS; full SDK6,565 tests/458files, wasm19 and host13 PASS. Eight-venue artifact2,707,883bytes.

### Final eight-venue checkpoint

- All19/19 gates PASS (eight public, eleven private), including corrected Crypto.com public/Phemex private reruns and Binance regressions. Median TS/Rust workerCPUms and retainedMB: phemex439.3/147.9,5.8/2.8; mexc1006.0/543.6,10.0/3.5; binance235.1/119.9,4.3/2.4; bybit464.1/302.8,6.2/2.5; bingx97.5/65.3,3.6/2.1; cryptocom87.4/65.5,2.0/1.5; okx1261.3/1234.1,5.2/3.5; weex227.5/190.1,5.2/2.3. Phemex steady307.9/64.0ms; Wasm2,707,883bytes under3MiB.
- Final checks PASS: workspace545 native tests, Phemex77 (account21/signing2/public8/trading34/queries12), native/wasm clippy, fmt/bans, SDK6,565/458files, wasm19/host13/harness49, typecheck44/lint28; exact-fingerprint secret history scan clean. All PR findings have fixes and independent clean re-review; latest landingbc2b61cc4a integrated. Push final checkpoint, wait final-head green CI, then merge PR1310 and cut a fresh follow-up branch.
- Latest landing follow-up `bc2b61cc4a` adds MEXC parity tests/fixture only; its updated25-frame private gate and54 native tests PASS; production artifact unchanged. Final integration review pending/published with final CI checkpoint.

### BloFin landing integration

- Landing advanced to `d9db55921c` while final-head CI was running. Merged BloFin and retained both registrations, typed request matching/caller IDs, Phemex bootstrap and first-socket readiness, MEXC ordinals, incoming normalized BloFin WS request audits and elapsed memory settlement using real timers. Independent cross-reviews of replay/requests/venues/gate/engine/store/wasm/workflow are clean;54 harness tests and BloFin private TS smoke pass. Matrix expands to21 gates (nine public, twelve private).
- Combined nine-venue artifact2,868,430bytes within3MiB. Phemex public gate PASS on it: TS/Rust CPU411.7/138.5ms, steady283.9/59.2ms, retained5.8/2.8MB. Full matrix and renewed final-head CI follow this stable merge.

### Final nine-venue validation

- All21/21 replay gates PASS (nine public, twelve private) on source4cc11fc375 and the2,868,430byte artifact. Median TS/Rust workerCPUms and retainedMB: phemex411.7/138.5,5.8/2.8; blofin204.8/189.9,3.4/2.1; mexc1171.1/670.3,10.1/3.6; binance183.3/80.1,4.3/2.4; bybit232.3/162.4,6.2/2.6; bingx93.1/66.5,3.6/2.1; cryptocom70.9/63.6,2.0/1.6; okx627.0/569.8,5.2/3.5; weex112.8/80.1,5.2/2.3. Phemex steady283.9/59.2ms.
- Workspace618 native tests (Phemex77), SDK6,567 tests/458files, wasm20/host15/harness54, fmt/bans/native+wasm clippy and typecheck44/lint28 PASS. Independent merge reviews are clean; latest landingd9db55921c integrated. Push this checkpoint; merge only after renewed final-head CI is green.

### Backpack landing integration

- Landing advanced to `b265a0bab0` during renewed CI. Adopted its consolidated shared Tape lifecycle and strict Backpack signing/audits; retained Phemex typed REST matching/HMAC/WS auth, explicit bootstrap, physical-socket startup seed fence, shared-URL auth routing and independent account oracle. Tests caught and fixed signed-envelope params alias loss and a startup fence waiting on a future stream subscription. Incoming dispatch topic checks remain strict; both startup regressions now exercise shared Tape directly. Cross-review clean;82 harness tests,25 host tests and21 wasm tests PASS. Matrix expands to23 gates.
- Ten-venue artifact initially exceeded3MiB by6,865bytes. Added the established per-crate size profile for Phemex; rebuilt artifact3,127,823bytes within3MiB. Phemex public gate PASS: TS/Rust CPU406.9/144.9ms, steady285.5/61.0ms, retained5.8/2.8MB. Native668 tests, native/wasm clippy, fmt/bans, typecheck44/lint28 PASS; remaining matrix/SDK and final-head CI follow this checkpoint.

### Final ten-venue validation

- All23/23 replay gates PASS (ten public, thirteen private) on source88e0a1e7f4 and the3,127,823byte artifact. Median TS/Rust workerCPUms and retainedMB: phemex406.9/144.9,5.8/2.8; backpack204.3/130.2,3.3/1.8; blofin231.3/200.2,3.4/2.1; mexc1160.0/642.2,10.1/2.9; binance274.5/138.1,4.3/2.4; bybit603.2/516.3,6.2/2.6; bingx163.5/101.9,3.6/2.2; cryptocom196.7/164.1,2.0/1.6; okx1675.1/1402.9,5.3/3.6; weex209.8/151.1,5.2/2.4. Phemex steady285.5/61.0ms.
- Workspace668 native tests (Phemex77), SDK6,567 tests/458files, wasm21/host25/harness82, fmt/bans/native+wasm clippy and typecheck44/lint28 PASS. Independent merge reviews are clean; latest landingb265a0bab0 integrated. Push this checkpoint; merge only after renewed final-head CI is green.

### Bitget landing integration

- Landing advanced to `4e468ed2e5` during CI. Merged its indexed routing/reconciliation, constructor/worker-buffer measurement fixes and throughput profiles. JSON selector merge preserves PathOmitLists and adds Envelope alternatives;51 engine tests including2 compatibility controls PASS. Retained all incoming tests and Phemex hooks; fixed missing Crypto params alias in generic factory and a delayed-auth notification race with actual immediate/delayed Phemex bootstrap regressions. Independent cross-reviews clean; native749, SDK6,567/458files, wasm25/host25/harness123, fmt/bans/clippy/typecheck44/lint28 PASS.
- Eleven-venue artifact3,775,242bytes; measured aggregate budget4MiB follows landing's additional venue/throughput settings. Runtime CPU/memory comparisons remain strict. Mandatory local Phemex/Binance public/private and Bitget private PASS; Bitget public output/memory PASS but CPUFAIL (TS561.7/Rust585.4ms, steady353.6/388.9ms). No gate relaxation. Investigating a bounded Bitget ticker-index cache using the existing Store indexed-update API before publication. Final CI will execute the complete25-gate matrix.

- Bounded Bitget ticker-index cache independently reviewed clean: rebuilt on both seed-table replacements, last-duplicate semantics retained, stale/unknown slots skipped; production-handler reorder/duplicate/removal regression added. Bitget78 native tests and all-target clippy PASS. Indexed artifact3,775,862bytes; replay output/memory PASS but CPU remains FAIL (TS595.7/Rust734.6ms, steady402.8/505.0ms). Cache alone is insufficient; investigating exact-semantic numeric decoding. No final gate claim or threshold change.

- Decoder investigation excluded a parser dependency: no useful fixed-workload numeric gain and no rounding changes warranted. The indexed profile eliminates the old binary-search hotspot and identifies repeated decoding/freeing of empty WASM action strings. Added optional `wsMessageActions` returning undefined when empty; legacy String API and older-host fallback retained. Native/actual-WASM and host controls verify ordered nonempty actions, single draining, receiver binding and disposal. Independent cross-review clean. Workspace751 native, hostJS28, SDKWasm25, harness123, fmt/bans/native+Wasm clippy, typecheck44/lint28 PASS. Artifact3,776,342bytes within4MiB; strict fixed-fixture CPU measurement pending.

- Optional-actions fixture measurement: totalCPU PASS (TS1623.1/Rust1406.3ms), retainedmemory PASS (5.7/2.5MB); steadyCPU still FAIL (TS1054.4/Rust1093.6ms). Preserve this failure; no unchanged rerun or gate relaxation. Next bounded experiment: optional strict-UTF8 byte entry using native TextEncoder instead of generated per-character string copying, with bounded scratch storage, unchanged action semantics and both existing fallback APIs. Independent plan/implementation review and fixed-fixture measurement required.

- Added optional `wsMessageBytes`: native TextEncoder and typed-array copying replace the generated per-character string-transfer loop; strict UTF-8 validation precedes events/draining. One bounded4KiB scratch buffer, complete-read fallback, reentry/exception guards, Unicode/lone-surrogate and large-frame controls preserve all three APIs. Independent source/actual-ABI/generated-binding review clean. Workspace752 native, hostJS31, SDKWasm25, harness123, fmt/bans/native+Wasm clippy and typecheck44/lint28 PASS. Artifact3,776,847bytes; fixed-fixture performance pending.
