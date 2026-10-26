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

### Hyperliquid landing integration after quota restoration

- Before integration, the reviewed UTF-8 byte path passed the previously failing Bitget public gate: TS/Rust CPU684.6/578.9ms, steady468.4/395.4ms, retained5.7/2.6MB on3,776,847byte eleven-venue artifact. No threshold change or unchanged retry.
- Merged latest landingc5bc2c4adf/Hyperliquid5c47683969. Kept incoming optional per-venue WASM features,1.2MiB single-venue budget and parallel CI matrix; added Phemex feature/factory/build flag/matrix row. Incoming shared Tape, signed-request/write/subscription audits, independent oracles, finite memory validation and all new protocol behavior retained. Preserved Phemex auth/bootstrap, first-physical-socket market seed fence, delayed-auth wake, strict depth/caller IDs and both reviewed fast WASM APIs.
- Incoming OrderByClientId query remains explicitly unsupported for Phemex, with a no-I/O regression. Independent cross-reviews of WASM/features/build/workflow/replay/Tape/queries are clean;158 harness tests including63 Tape tests PASS, bans/fmt/workspace clippy PASS. Full native/SDK/combined-WASM checks and renewed mandatory local gates follow. Final CI must run27 replay gates (12public,15private) before merge.

- New Phemex public bundle gate PASS: TS/Rust CPU427.1/134.0ms, steady294.7/52.6ms, retained5.9/2.8MB;719,238bytes below1.2MiB. Binance public/private PASS (CPU297.9/162.2ms, steady179.4/78.4ms, retained4.3/2.4MB). Integrated full SDK6625/459files, native819, host31/SDKWasm25/harness158 and remaining required source checks PASS.
- Strict private audit exposed TS terminal reconciliation redundantly reading a symbol after its full snapshot; a negative control proved2contracts overwritten by stale1. Independent review caught a P1 in the first proposed symbol-only optimization: it could suppress the missing Short leg when Long already existed. Final fix tracks queued native affected-side unions; unknown sides still recover, Merged requires OneWay coverage, and scoped recovery cannot overwrite nonzero full-snapshot IDs. Original unknown-side recovery assertions retained;12 new regressions cover stale overlays, both hedge legs, unions, one-way, signed quantities and failures.51 Phemex TS tests/10files, targeted ESLint/package typecheck and independent re-review PASS. KNOWN_TS_FIXES documents the correction; no request normalization or threshold change. Final private parity/expanded SDK checks follow.

### Final per-venue local checkpoint

- All6/6 local replay gates PASS: Phemex, Binance and Bitget public/private. Phemex private14frames now passes unchanged strict request/write/subscription and account comparisons after the reviewed TS terminal recovery fix. Public TS/Rust CPUms, steadyms, retainedMB: Phemex427.1/134.0,294.7/52.6,5.9/2.8; Binance297.9/162.2,179.4/78.4,4.3/2.4; Bitget2130.5/1966.2,1465.8/1396.6,5.5/2.7. Phemex artifact719,238bytes under1,258,291.
- Native819 (Phemex77), hostJS31, SDKWasm25, harness158, bans/fmt/native+Wasm clippy, typecheck44/lint28 and51 focused Phemex TS tests PASS. Full SDK6625/459files passed before the terminal-recovery fix; its expanded final-source suite is running during publication. All concrete adversarial findings have independently reviewed fixes. Push this stable source; merge PR1310 only after final-source SDK validation and all27 final-head CI replay gates pass, then cut a fresh follow-up branch.

### Bitrue and transport landing integration

- Merged landingfc18c55874 (Bitrue1314 and transport1313). Retained13 optional venue bundles, unchanged1.2MiB budget and29 CI replay gates (13public,16private). Preserved all Phemex HMAC/bootstrap/oracle/caller-ID controls, independent transport protocol audits and reviewed WASM fast entries alongside incoming native/JS routing, request deadlines and disposal.
- Independent conflict and cross-reviews are clean. Phemex's shared URL now carries explicit Public/Private socket roles. Compilation caught incoming Bitrue's old exhaustive socket pattern; both Bitrue sockets now also carry typed roles, with role-selecting startup regressions and independent clean review. No missing payload recovery hint is needed for their one feed per role.
- Prior expanded final-source SDK suite passed6637tests/460files. Renewed merged-source checks so far:164 harness tests,40 hostJS,25 SDKWasm, typecheck44/lint28 PASS; native tests/clippy and full SDK run continue. The combined WASM build passes; fresh per-venue mandatory replay gates follow once compilation and SDK work finish. Merge remains gated on all29 final-head CI replays and green source checks.

### Final thirteen-venue publication checkpoint

- All6/6 renewed local replay gates PASS on sourced058facb1c: Phemex, Binance and Bitget public/private. Median public TS/Rust CPUms, steadyms, retainedMB: Phemex424.3/136.0,291.5/58.2,5.8/2.8; Binance274.1/150.9,161.3/72.5,4.3/2.4; Bitget1986.5/1864.2,1400.6/1272.7,5.5/2.6. Phemex bundle720,371bytes under1,258,291. Private frame counts14/14/12; correctness/signing/write/subscription/account audits PASS. No threshold changes or unchanged retries.
- All merged-source checks PASS:905 native tests (Phemex77), full SDK6857/467files, hostJS40, SDKWasm29, harness164, bans/fmt/native+Wasm clippy, typecheck44/lint28; combined and single-venue builds. All concrete adversarial findings have independently reviewed fixes; integration/role follow-up reviews clean. Push stable checkpoint; merge PR1310 only after all29 final-head CI replay gates and required checks pass, then cut a fresh follow-up branch from landing.

### Bitget inverse/trailing parity landing integration

- Landing advanced during CI to d172a666b1 (Bitget parity1317). Merged incoming inverse sizing, UTA collateral balance tail, trailing contracts and new private fixture/integration suite; retained Phemex feature/factory/audits and both WASM fast APIs. Two test conflicts: retain the typed Bitrue Private startup selection and keep both Bitget inverse product coverage and ticker-cache/independent-feed regressions. Bitget cache compatibility and harness cross-review clean; shared frame/factory compatibility review also clean: Phemex keeps the legacy32-byte balance payload, optional collateral is additive, and inverse-only amount conversion leaves Phemex untouched.
- Matrix now30 replays (13public,17private), plus4 Bitget native-contract integration tests. The new inverse fixture intentionally excludes known-broken TS inverse WS account/position paths; incoming native controls independently exercise them. Renewed source checks and7 mandatory/local control replays follow; no Phemex implementation behavior changed.

### Final Bitget-parity publication checkpoint

- All7/7 local replay gates PASS on source654c686ba2: Phemex/Binance/Bitget public and private, plus Bitget inverse private. All4 incoming UTA/Classic trailing contract integration tests PASS. Public median TS/Rust CPUms, steadyms, retainedMB: Phemex337.6/84.3,234.8/24.6,5.8/2.8; Binance192.0/91.9,109.4/35.6,4.3/2.4; Bitget924.3/700.8,592.6/471.4,5.5/2.6 on the incoming39,457frame fixture. Phemex bundle721,011bytes below1,258,291. No gate relaxation or unchanged retry.
- All source checks PASS:976 native (77Phemex), SDK6858/468files, hostJS41, SDKWasm29, harness164, bans/fmt/native+Wasm clippy, typecheck44/lint28, combined+single-venue builds. Prior71232b3a2e CI completed all29 replays and source checks successfully; newer merged source requires30 final-head replay gates plus4 contract controls and generalCI before merge. All concrete findings independently re-reviewed clean.

### Woo landing integration

- Landing advanced during CI to6945ee83b4 (Woo1316). Merged14 features/factories/build allowlist and Woo-specific request/auth/oracle hooks, preserving Phemex bootstrap/strict audits, all host fast APIs and previous Bitget inverse controls. Independent harness, WASM/shared option and root configuration cross-reviews clean. New optional max_slippage_pct defaults absent; both explicit Phemex order constructors setNone without changing venue behavior. Incoming removal of unused hmac_sha512 retained; Phemex uses hmac_sha256.
- Matrix32 replay gates (14public,18private) plus4Bitget contract controls.170 harness tests PASS; renewed source/build checks underway. Prior CI reported a BingX public gate failure; diagnostics must be examined and any concrete defect resolved before final merge.

### BingX steady-CPU regression fix

- Prior edb68f2039 CI completed17/18jobs green; only BingX public steadyCPU failed (TS43.8/Rust47.2ms), while output/totalCPU/memory passed. Independent plan and implementation audits approved the bounded per-crate release opt-level z→3 change: existing ticker indexes/sorting/numeric semantics remain intact, no threshold relaxation or unchanged retry.
- Final-profile combined build and41host/31SDKWasm tests PASS. BingX fixed-fixture public gate PASS: TS/Rust totalCPU182.5/128.6ms, steady97.9/72.1ms, retained3.6/2.2MB;839,731bytes below1,258,291. Private38frames PASS unchanged strict correctness. Source checks alsoPASS:1086native (77Phemex), SDK6905/472files, harness170, bans/fmt/native+Wasmclippy/typecheck44/lint28. Renewed Phemex/Binance/Bitget/inverse gates and4contract controls follow on frozen source.

### Final fourteen-venue publication checkpoint

- All9/9 local replay gates PASS on sourcef22df864e9: Phemex/Binance/Bitget/BingX public/private plus Bitget inverse private; all4Bitget contract integration tests PASS. Public median TS/Rust CPUms, steadyms, retainedMB: Phemex462.2/116.9,327.8/42.6,5.8/2.8; Binance271.2/148.9,159.9/70.8,4.3/2.4; Bitget2067.4/1911.2,1391.8/1151.1,5.5/2.6; BingX182.5/128.6,97.9/72.1,3.6/2.2. Measured Phemex721,647/Binance757,253/Bitget933,915/BingX839,731byte bundles all below1,258,291. No thresholds changed and no unchanged retries.
- All source checksPASS:1086native(77Phemex), SDK6905/472files, hostJS41, SDKWasm31, harness170, bans/fmt/native+Wasmclippy, typecheck44/lint28; final-profile combined and single-venue builds. Independent phase/adversarial/integration/profile reviews are clean, with concrete findings and fixes posted on PR1310. Push final checkpoint; merge only when all32 final-head CI replay gates plus4contract controls and required checks are green, then cut a fresh follow-up branch from landing.

### Lighter landing integration

- Landing advanced at publication to31ec401f60 (Lighter1319). Merged15 optional features/factories/build entries and scoped independent Lighter signing/oracle hooks, preserving every Phemex request/bootstrap/oracle/fast-API control and BingX throughput profile. Harness186 controls PASS; independent harness/config/crypto reviews clean. Crypto changes remove two unused Lighter hash exports/fixtures while retaining Fp5/Schnorr vectors and Phemex HMAC; no shared model/native-host change.
- Independent wrapper review caught P1: incoming unconditional caller-ID guard rejects valid Phemex/Binance/PM IDs because their Rust wrapper rules were absent. Fix adds all three exact TS honoring/mint rules and14 no-I/O regressions while preserving Lighter rejection; focused testsPASS and independent re-reviewclean. Phemex broker constant now lives in its types module, with API re-export preserving existing callers. Final matrix34replays(15public,19private)+4Bitgetcontractcontrols. Verify affected Phemex/Lighter/crypto native, combined host/WASM boundary, wrapper regressions/typecheck/lint, and mandatory Phemex/Binance replay pairs; full final-source workspace/SDK and34matrix coverage remain required in CI.

### Final Lighter-integration publication checkpoint

- All6/6 fresh local replay gates PASS on source038e203f7d: mandatory Phemex/Binance public/private plus Lighter public/private. Public median TS/Rust CPUms, steadyms, retainedMB: Phemex384.2/96.7,263.1/30.2,5.8/2.8; Binance252.8/137.0,150.2/67.5,4.3/2.4; Lighter701.1/497.6,469.8/332.9,3.0/1.9. Single-venue Phemex721,647/Binance757,253/Lighter969,129byte bundles all below1,258,291. Prior BingX/Bitget/inverse and4contract controls remain passing atf22df864e9; current final CI must cover the complete34replays+4controls.
- Current affected-source checksPASS:190Phemex/Lighter/crypto native(77Phemex),532TS/31files including14newcaller-ID regressions,41host/37SDKWasm,186harness, bans/fmt/workspaceclippy/Wasmclippy/typecheck44/lint28, combined+single-venue builds. Last whole-workspace/fullSDK local evidence:1086native and6905SDK/472files before Lighter; final current-source full suites remain CI requirements. All concrete findings have independently reviewed fixes; posts record the P1 and clean re-review. Push checkpoint, wait final-head greenCI, merge PR1310 into feat/rust-rewrite and cut a fresh follow-up branch.

### Green fifteen-venue CI checkpoint

- Exact-head d768e0cccc CI39353252432 completed20/20jobs success: all34/34 replay gates (15public,19private),4/4Bitget contract controls, bans/fmt/nativeclippy/1170workspace-native-tests/wasm32. Phemex CI public CPU354.2/97.0ms, steady232.3/25.5ms, retained5.7/2.6MB TS/Rust;721,721byte artifact. BingX CI steady43.3/33.4ms and public/privatePASS confirm the throughput fix. GeneralCI/secret scans were still running when landing advanced toOrderlyf66bdc26f9, creating new registration conflicts. Preserve this green proof; integrate only the new delta and require renewed final-head acceptance before merge.

### Orderly landing integration

- Landingf66bdc26f9 adds Orderly's seven broker names across mainnet/testnet. Merged16 features/builds/factories and retained the reviewed Phemex/Binance/PM client-ID rules, broker mints, all fast APIs and BingX throughput profile. Independent wrapper/host/harness/config/shared-engine/native reviews are clean. Native position-ID tails and deactivation are opt-in; Phemex keeps legacy layouts and behavior. Root lockfile preserves bothPhemex andOrderly packages without dependency-version refresh.
- A real automatic-merge defect was fixed: audit.connected had been called twice, creating a phantom Orderly socket. Keep one call with generation, backward-compatible with Woo.215 harness testsPASS; affected shared/native/wrapper/combined checks underway. Final matrix49replays (16public,33private, including14Orderly broker/network variants), plus4Bitget controls. All three prior d768e0cccc CI runs now completedsuccess, including34replays/1170native and generalCI/secret scans; newer merged source still needs final acceptance.

### Startup memory diagnostic checkpoint

- Renewed Phemex public replay twice stopped before startup/performance measurement at the unchanged1s backing-buffer baseline guard (Node24, thenCI-matchingNode22). Preserved both failed logs; no parity/CPU/memory comparison was produced. Temporary scalar sampling showed stable buffers and ordinary GC after an86ms firstGC; independent review found no new recurring loader/import allocator. Removed temporary trace.
- Added independently reviewed failure-only diagnostics (attempt/equal counts and longestGC) without changing timeout/quiet/equality/pacing/returned sample.215harness controlsPASS. The first untraced full gate after diagnostics nowPASS: Phemex TS/Rust CPU451.5/130.3ms, steady316.7/58.0ms, retained5.7/2.7MB,722,160bytes. Continue private/Binance/Orderly controls with Node22; no valid performance result was discarded and no guard/threshold relaxed.

### Bounded startup-settling fix

- Binance also exhausted the original1s baseline budget before any score:11changing samples, longestGC214.8ms. Diagnostic replays showed stable counters once cold setup completed; source audit found no recurring allocator. Increased only the settling deadline to5s, retaining100msquiet, three equal samples, the first qualifying actual sample, and failure for continuously changing counters. Score thresholds/arithmetic remain unchanged.
- Independent plan/implementation review confirms symmetric TS/Rust CPU timing starts after baseline and is captured before final collection. Two deterministic controls reject the old1s cold cleanup, return exactly the first qualifying sample at1600ms with5s, and still reject continuous growth at5100ms.217harness testsPASS. Normal untraced Binance public gate nowPASS: TS/Rust CPU287.4/159.0ms, steady180.7/80.4ms, retained4.2/2.3MB. Failed setup logs remain preserved; no valid score was selected/discarded. Finish Binance private/Orderly pair, then final49-gateCI.

### Final Orderly-integration publication checkpoint

- All6/6 local replays PASS: Phemex/Binance/Orderly public/private on Node22. Public medians TS/Rust CPUms, steadyms, retainedMB: Phemex451.5/130.3,316.7/58.0,5.7/2.7; Binance287.4/159.0,180.7/80.4,4.2/2.3; Orderly181.5/123.3,140.4/88.7,2.0/1.3. Phemex722,160/Binance757,702/Orderly788,584byte artifacts all below1,258,291. Earlier failed startup baselines and diagnostics are preserved; no valid performance comparison discarded. The5s bounded settling fix preserves quiet/equality and all score thresholds.
- All affected checksPASS:243engine/native-host/WASM/Phemex/Orderly native(77Phemex),458TS/24files,48host/52SDKWasm,217harness, bans/fmt/workspace+WASMclippy/typecheck44/lint28, combined+single-venue builds. Independent integration and settling-fix reviews are clean; all concrete findings have reviewed fixes. PreviousfullCI(d768e0cccc) passed34replays/1170native/generalCI/secret scans. Publish this source, require49replays+4Bitgetcontractcontrols and current full suites green inCI, merge only into feat/rust-rewrite, then cut a fresh follow-up branch.

### Bybit variants integration plan and audit

- Landing922b5fdc89 adds Bybit inverse/spot/demo and the documented Hyro profile. Preserve16venue crates and validate57replays (20public,37private), plus4Bitget contract controls. Independent scoped harness/JSON/WASM/model/wrapper reviews resolved conflicts, preserving Phemex streamed catalogs, strict caller IDs, TS oracle corrections, role/bootstrap audits, and all prior venue hooks. Engine JSON union received an additional independent review.
- Concrete integration findings: restore legacy wsMessage's String ABI while retaining optional fast APIs; balance seeking parser ancestry when skipping a list before the Phemex catalog; reject both newly shared Bybit execution controls during Phemex whole-command preflight rather than silently dropping them. Add absent defaults to internal constructors and a late-invalid two-item batch regression for each control.
- Retain incoming symmetric before-import retention baselines (loaded implementation modules count), strict nonnegative component validation, and structured failure diagnostics. Combine these with the reviewed5s settling budget/100msquiet/first qualifying actual sample, preserving longestGC diagnostics. Incoming stricter component validation supersedes the earlier loaded-code-baseline signed-delta policy; no clamp or score threshold relaxation. Independent fairness/guard review requested.
- Verify scoped shared/Phemex/Bybit native suites, full harness, wrapper/Phemex/Bybit focusedTS, typecheck/lint, bans/fmt/workspace+WASMclippy, combined host/WASM boundary tests, then sequential mandatory Phemex/Binance and incoming Bybit variant replay gates. Commit/push stable source, post findings, require all final-head CI jobs green before merge and cut fresh follow-up branch.

- Integration source checkpoint:289affected native testsPASS (78Phemex),237harnessPASS, nativeworkspaceclippyPASS, typecheck44PASS; remainingWASMclippy/lint/build/boundary/replay checks continue. FocusedSDK824/826passed; two default-loader test failures follow a first-test5s cold-import timeout under concurrent60file/native/typecheck work. Preserve the failed log and investigate using an isolated single-worker run before considering the source accepted. All concrete integration findings independently re-reviewed clean and posted in PR1310 comment6281545159.

### Renewed adversarial findings and bounded fixes

- P1: a stale hedge row could contribute context to a fresh opposite leg despite its position being fenced. Move temporal acceptance into a local semantic preview before sequence/authority commits; only accepted rows supply context. Preview native versions and actual/simulated presence so a newly staged live row cannot be erased by a later untimestamped flat row. Regression controls cover cross-/same-frame leverage contamination, same-frame live→flat0, and atomic missing-mode rejection/corrected same-sequence retry. Independent plan/implementation reviews caught the simulated-presence edge; final rereviewclean.
- P2: Phemex harness authentication now resets per attempt and is scoped to physical socket generation; only explicit successful matching ACK permits subscription/readiness. Nine new controls cover valid shared-URL bootstrap, reauth, reconnect/staleACK, and failed/missing/wrong/cross-socketACKs. Independent rereviewclean;246fullharnessPASS.
- The incoming actual-WASM quiet-action test intercepted the legacy method even though the host prefers bytes. Parameterize it to verify the preferred bytes entry AND legacy-only fallback, retaining exact empty-return/ticker/error assertions. Root review confirms the original String ABI and assertions remain intact.
- Public review additionally found explicit candle limit lost when from was supplied; follow TS limit precedence and retain default500/cap1000 with request regressions. Private/candle native verification and combined75actual-WASM tests follow. Findings posted as PR1310 comment6281621134; no live credentials or score thresholds involved.

- Final candle boundary additionally follows TS zero-as-absent semantics and rejects reads before HTTP during an active429pause. Three request controls cover positive/capped/default/zero/from precedence and actual429 deadline−1/deadline admission. Independent final delta reviewclean. Initial compilation required an explicit HashMap<String,(u64,bool)> annotation for preview state; added without behavioral change and rerunning affected checks.

- Post-fix verificationPASS:295affected native (84Phemex),246harness,51hostJS,75actualSDKWASM, bans/fmt/workspace+WASMclippy, typecheck44/lint28; finalcombinedbuildPASS. FocusedSDK coverage824initialpass plus3/3isolatedloaderpass covers826distinct tests; preserve initial timeout/cascade log, independent source review confirms no loader regression. All concrete findings have independently reviewed fixes. Begin mandatory Phemex/Binance public/private replays on frozen source, then Bybit linear/inverse/spot/demo/Hyro controls; finalCI57+4remainsrequired.

### Bybit integration replay checkpoint

- All14/14localreplaysPASS on d6abb7f782: Phemex/Binance public/private plus Bybitlinear/inverse/spot/demo/Hyro public/private. PhemexCPU350.5/86.0ms,steady240.2/25.2ms,retained19.8/2.9MBTS/Rust under symmetricbefore-importmeasurement;729,954bytes. Binance158.5/88.2ms,80.0/35.0ms,18.5/2.4MB;763,690bytes. Bybit927,548bytes; allbelow1,258,291. Allsource/boundarychecksabovePASS; findings/fixespostedPRcomment6281749759.
- KuCoin373cc09864 landed before publication. Nextboundedphase merges17feature/build/factoryregistration and KuCoin's request-connectID/welcome/settling hooks while retaining Phemexauthgeneration/HMAC/bootstrap/oracle andallsharedfastAPIs/callerIDs. Independentread-onlyplan/sharedunit/configreviewsareclean. Expected59replays(21public38private)+4Bitgetcontrols. VerifyaffectedKuCoin/Phemex/WASMnative, focusedSDK/harness/typecheck/lint, combinedboundary, mandatoryPhemex/BinancepairsandKuCoinpair; finalfullcurrentCIrequired.

### KuCoin landing integration checkpoint

- Merged373cc09864 with17optionalfeatures/builds/factories. Independent scoped reviews/cross-review clean: preserve all84Phemextests/fixes, threeWSAPIs/legacyStringABI, caller-IDrules/mints, exacttypedrequestmatching andgeneration-scopedauth. KuCoinvolatileconnectIDs propagate through every socket/request canonicalkey (includingretainedsocketshelper); token remains exact. Welcome pre-delivery and settlement hooks coexist with Phemex explicitbootstrap/firstphysicalfence/authsnapshot andOrderly/Bybithooks. No sharedengine/model/crypto production change.
- AffectedchecksPASS:186Phemex/KuCoin/WASMnative(84Phemex),479SDK/22files(serialworker,loader3/3pass),268harness,51host,76actualSDKWASM, bans/fmt/lockedmetadata/workspace+WASMclippy/typecheck44/lint28; combined17-featurebuildPASS. Allconflictsresolvedandreviewed. RenewedPhemex/Binance/KuCoinpublic+privatepairsfollow; final59replays+4Bitgetcontrols andallcurrentfullCIchecksrequired beforemerge.

### Final seventeen-venue publication checkpoint

- All6/6renewedlocalreplaysPASS on8b835eed86: Phemex/Binance/KuCoinpublic+private. MedianpublicCPUms/steadyms/retainedMBTS/Rust: Phemex359.8/101.5,242.9/32.8,19.8/2.9; Binance268.7/147.6,159.6/76.4,18.5/2.4; KuCoin576.1/377.2,378.1/240.3,21.4/3.9. Bundles729,954/763,690/756,781bytesbelow1,258,291. Priord6abb7f782all14Bybit/Phemex/BinancecontrolsPASS; finalcurrentCIcoversall59replays+4Bitgetcontrols. No validscorediscardedorunchangedretry.
- Allsource/boundarychecksabovePASS, andallconcretefindingshaveindependentlyreviewedfixes. Publishthischeckpoint/PRreviewfindings, requireallfinal-headRust/general/secretchecksgreen, thennormalmergePR1310into feat/rust-rewrite andcutfreshfollowupbranch. PreservefinalCIproofinPRcomments/mergecommit ratherthanrestartCIwithadoc-onlycommit.

### CI fixture-isolation correction

- Published2932a6b840CI passed all1,460workspace-native tests, source checks and completed venue replays, but BingXstoppedbeforeitsreplay in the sharedharness: actualWASMlinear-memorytest sawarrayBuffers delta−16,777,216instead0. Exactdropmatches the precedingtest's16MiBArrayBuffer, nowunusedexceptbyteLength afterKuCoinchangedthatfixturetosyntheticcounters. Itsbackingstorewascollectedduringthenexttest.
- ReplacedonlytheunusedallocationwithconstframeBytes=16*1024*1024, preservingidenticalcounterarithmeticandassertion. TheactualWASM allocation/arrayBufferszero/exactonceassertions, memorysampler, scorevalidation andallreplayruntimecodeareunchanged. Independentplan+implementationreviewsCLEAN;268harnesscontrolsPASS. Commit/pushthisprecisetest-isolationfix, thenrequireallfinal-headCIgreenwithoutunchangedretryorpolicychange.

### Green seventeen-venue CI and AsterDEX integration plan

- Exacthead9cde01848aCI fullyPASS: Rust39358886204success22/22jobs,59/59terminalreplayPASS(21public38private),4/4Bitgetcontrols,1,460native(84Phemex). BothgeneralCI39358886135/39358881822andsecretscanssuccess. Independentevidenceauditclean. FinalPhemexCI public/privatePASS;CPU293.3/74.7ms,steady187.8/19.6ms,retained19.7/2.9MB,729,964bytes. Fixtureisolationfixconfirmedacrossmatrix.
- AsterDEXd9aa597fd3landedbeforemerge,changingnewsharedSTP/cancelcommands,tickeridentity/currencytailsandroutinghints. Nextboundedphasepreserves18feature/factory/buildregistration andallPhemexfixes/hooks, addsabsentSTPconstructordefaultsandrejectsunsupportednewexecutioncontrols/commandswithoutwrites, auditsoptionalsharedlayout/routingsemanticsandtypedroles. Independentphaseplans/reviewsrequested;verifyaffectedshared/Phemex/Aster/native/host/WASM/SDK/harness/typecheck/lint,thenmandatoryPhemex/Binance+AstercontrolsandfinalcurrentCIbeforelanding. Preservegreen59gateproof;donotclaimitcoversthenewdelta.

### AsterDEX integration source checkpoint

-18optionalfeatures/builds/factoriespreserved;independentharness/WASM/routing/model/wrapperreviewsandPhemexcross-reviewCLEAN. IncomingoptionalHTTProute/timeoutmetadataisconsumedbyhostsandneverbecomesheaders;ordinaryPhemexrequestskeepNone/defaultaccountpolicy. Optionalorphan-tickeridentity/currencytailsleaveknown-market/aggregatePhemexlayoutsunchanged. Exactsyntheticgitleaksfingerprintsfrombothlanesretainedwithoutbroadignore.
- Phemexconstructorsdefaultstp_modeNone;allthreeexplicitSTPmodesrejectduringwholebatchpreflight. FiveincomingunsupportedcommandsandpreviouslyfallthroughFetchLeverageerror-completewithoutIO;PhemexSDKexistingread/cancelpathsremainunchanged. Addedlate-invalidSTP+sixcommandregressions;independentimplementationreviewclean.
-316affectednativePASS(86Phemex),346SDK/25filesPASS,278harnessPASS,typecheck44/lint28PASS,bans/fmt/lockedmetadataPASS. Native/WASMclippyandcombined18-featurehost/SDKWASMboundarycheckscontinue;subsequentmandatoryPhemex/Binancepairs+Asterpublic/V1/V3and3nativecontrolsplanned. Finalmatrix62replays(22public40private)+7controls(Bitget4,Aster3);requireexact-headfullCIgreenbeforelanding.

- FinalAstersourcechecksPASS:bans/fmt/lockedmetadata,316affectednative(86Phemex),346SDK,278harness,workspace+WASMclippy,typecheck44/lint28,combined18-featurebuild,57hosttests,76actualSDKWASM. AllboundedreviewsCLEAN. Runtime9fa692238eisfrozenfor7localreplays(Phemex/Binancepairs,Asterpublic/V1/V3)and3Astercontractcontrols;final62replays+7controls/currentfullCIstillrequired.

### Final eighteen-venue publication checkpoint

- All7/7freshlocalreplaysPASS: Phemex/Binancepublic+private,Asterpublic/V1private/V3private;3/3AstercontractcontrolsPASS. PublicmedianCPUms/steadyms/retainedMBTS/Rust: Phemex466.4/128.8,320.8/48.0,20.0/3.0;Binance231.7/145.7,128.6/74.9,18.6/2.4;Aster197.6/107.3,72.2/56.7,23.2/2.0. Bundles760,201/793,847/1,036,120bytesbelow1,258,291. Source/boundarychecksabovePASS;allconcretefindingsindependentlyreviewedclean.
- PublishthischeckpointandupdatedPRproof,require62replays+7controls/allcurrentfullCIgreen,normalmergePR1310intofeat/rust-rewriteandcutfreshfollowupbranch. Prior9cde01848afull59gateCIproofpreserved. No scorethresholdchangeorunchangedvalidscoreretry.

### Source-buffer accounting test and Kraken plan

- bc60de0326RustCIcompleted23jobswithonlyAster'ssharedharnessstepfailedBEFOREreplay: actualWASMunitexpectedJSbufferdelta0butobserved−6,869bytes, exactlythethen-currentmemory.test.mjsfilelength. ThisisloadedsourcebufferGC, separatefrompriorremoved16MiBfixtureallocation. No actualvalidperformancegatefailed/wasdiscarded.
- Independentplan+implementationreviewsapproveunit-onlycorrection: assertnoobservedJSbufferGROWTHduringWASMallocation, retainexactsignedheap+buffer+WASMaggregate and addexactWASMparametercontribution==bytes. No tolerance/retry/GCflags; memory.mjs/productionvalidators/scoringunchanged.278harnessPASS.
- Kraken7da56dc8a5 landedbeforepublication. Nextboundedphasepreserves19feature/build/factoryregistration+Krakenprotocol/requestoracles alongsideallPhemex/Aster/Bybit/Orderlyhooks; no incomingengine/model/crypto change. Auditwrapper/caller-ID/unit/routingautomerge,thenaffectedKraken/Phemex/WASMnative/harness/SDK/typecheck/lint/combinedboundary+mandatoryPhemex/Binance/Krakenpairs. Expected64replays(23public41private)+7controls;requireexact-headCIgreenbeforelanding.

### Kraken integration source checkpoint

-19optionalfeatures/factories/buildentriespreserved;independentharness/WASM/wrapper/configreviewsandcross-reviewCLEAN. Sharedengine/model/cryptoandall86Phemexnativefixesunchanged. Krakenwhole-resultoracleistape-derived/venue-scoped/appliedonce;book-readinesshookusesrealTimeout,preservingAsterclock+lazyvalidators,KuCoinwelcomes,Bybit/OrderlyhooksandPhemexbootstrap/auth/strictIDs. Bothnarrowgitleaksfingerprintsetsretained.
-184affectednativePASS(86Phemex),140SDK/21filesPASS,290harnessPASS,bans/fmt/lockedmetadata/workspace+WASMclippy/typecheck44/lint28PASS. Combined19-featurebuild/host/SDKWASMcheckscontinue;thenmandatoryPhemex/Binance/Krakenpublic+privatepairsandfinal64replays+7controls/exact-headfullCIbeforelanding. Memoryunitfixremainsreviewedandproductionmemory/scoringunchanged.

### Final nineteen-venue publication checkpoint

- All6/6freshlocalreplaysPASSon055ab85ce0:Phemex/Binance/Krakenpublic+private. PublicmedianCPUms/steadyms/retainedMBTS/Rust:Phemex448.6/139.8,313.9/62.1,20.0/3.0;Binance252.6/147.0,144.7/70.7,18.7/2.4;Kraken979.0/165.0,718.4/89.5,18.0/2.1. Bundles760,201/793,847/892,717bytesbelow1,258,291. Allsource/boundarychecksabovePASS;independentintegrationreviewsCLEAN.
- Publishthissource/updatedproof,require64replays+7controls/allcurrentfullCIgreen,normalmergePR1310intofeat/rust-rewriteandcutfreshfollowupbranch. No productionmemory/scoringchange;unitGCcorrectionretainsstrictobservedgrowth/exactarithmeticchecks. Priorallgreen9cde01848a59gateproofpreserved,notclaimedfornewdelta.

### Apex Omni integration plan

- ApexOmniecb313026f landedwhile07c93f8459CIwasrunning. It addsonevenue/scopedwrapper+harnesspaths, no sharedengine/model/cryptoproductionchange. Preserve20feature/build/factoryregistration,exactPhemexcallerIDs/auth/bootstrap/oracle/memoryfixesandallpriorvenuehooks;independentboundedreviewsrequested. Expected66replays(24public42private)+7controls.
- RequiredPhemex/BinancegatesalreadyPASSonunchangedvenuecode;verifynewaffectednative/SDK/harness/typecheck/lintandcommit/publishstablemerge promptly. Runcombined/localcontrolsinparallelwithfinal-headfullCIacceptance;all66replays+7controls/source/general/secretchecks mustgreenbeforemerge. ThisavoidsserializingunaffectedlocalrepeatsoverfullCI,withoutomittingfinalexact-headgates.

### Apex Omni source publication checkpoint

-20optionalfeatures/factories/buildentriespreserved;independentharness/WASM/wrapperreviewsandcross-reviewsCLEAN. AllPhemex/Binance/PMcaller-IDrules/mints/unconditionalguardsremainexact;sharedensureHostpreservesoneinitialization/startup/disposalguardsandApex-onlyaccount-before-startbehavior. Apextypedoptions/account/historyscopeiscorrect;all3WSAPIs/routing/newcommandsretained. No sharedengine/model/crypto/Phemexsourcechange.
-191affectednativePASS(86Phemex,99Apex,6WASM),181SDK/20filesPASS,308harnessPASS,bans/fmt/lockedmetadata/workspace+WASMclippy/typecheck44/lint28PASS. Commit/publishstablemergeforfullCI66replays(24public42private)+7controls,whilecombined20-featurehost/SDKWASMboundaryandlocalPhemex/Binance/Apexreplaysruninparallel. Noneofthese pendingfinalgatechecksisbeingclaimedcomplete.

### Green twenty-venue checkpoint and Paradex integration plan

- Exacthead d414561eb4f56171e1ad73f6bfc4742e6fcd0b92 RustCI39362678356 PASS25/25jobs,66/66replays(24public42private),7/7controls(Bitget4,Aster3),1,766native including86Phemex. Independent exact-run/log-ID/artifact audit CLEAN. Phemex publicCPU573.6/148.6ms,steady377.7/45.2ms,retained19.9/2.9MB,763858bytes;private14framesPASS. Local6/6Phemex/Binance/Apexpairs,combined20-featurebuild,57host/77actualSDKWASM testsPASS; independent evidenceauditCLEAN. GeneralCIstillrunning when Paradex landed; do not claimall-CIacceptance.
- Paradex1f9e9a5206 adds explicit binary ingress/Event::WsBinary, host transport negotiation, typed protocol replay and21stvenue. Nextboundedphasepreserves landingstructure, allPhemexprotocols/roles/callerIDs/fasttext+legacyStringABI andstrictmemoryscoring. Reviewengine/crypto cleanup andharness horizon/unit changes independently; resolve scopedconflicts then cross-review. Runaffectedshared/Phemex/Paradex/native/WASM/host/SDK/harness/bans/fmt/clippy/typecheck/lint, publishstablemerge promptly, combinedbuild+mandatoryPhemex/Binance/Paradexpairs parallelwithfullcurrentCI. Expected68replays(25public43private)+7controls,26Rustjobs; requireallcurrentCIgreenbefore normalmergeandfreshbranch.

### Paradex source publication checkpoint

-21optionalfeatures/build/factoryentriespreserved. IndependentscopedreviewsandcrossreviewsCLEAN acrossharness/WASM/JS/engine/native/wrapper/cryptocleanup. Fixed concreteautomergeomission: addPhemex tonewsecretfeaturecfgso single-venueprivateauthretainssecret. AllthreeexistingtextWSAPIs/legacyStringABIremainbyte-identical;separateexplicitbinaryevent/transportpreserves negotiatedbytesandnormalPhemextext/decompression. CallerIDrules/mints,ensureHostlifecycle,allpriorvenuehooksandstrictmemoryscoringremain. Retiredgenericcryptohelpershave no venuecallers;dynamicABIonlyindevoracles,starknet-core removed.
-326affectednativePASS(86Phemex,107Paradex,7WASM),444SDK/32filesPASS(238focused+206transport),331harnessPASS,bans/fmt/lockedmetadata/workspace+WASMclippy/typecheck44/lint28PASS. Previousd414 allthreeCIrunsnowfullysuccess(66replays+7controls),preservedasprecedingproofonly. Publishthisstablemergefor68replays(25public43private)+7controls/26Rustjobs;runcombined21-featurehost+SDKWASM/6Phemex-Binance-Paradexreplaysinparallel. Requireallcurrentlocal/fullCIgreenbeforelanding.

### Port-review lows plan (2026-10-21)

1. Encode the documented `ByIndexPrice` trigger for standalone and attached
   SL/TP; native account readback already supports it. Test exact wire fields
   and cached readback without altering other trigger defaults.
2. Pin correct daily candle `to`: native honors the caller; TS's previous-day
   shift drops valid bars. Record the deliberate correction and stale public-row
   startup finding. Retain cached cancel provenance and policy-controlled account
   polling; explain safety/load choices in the audit rather than guessing IDs.
3. Reproduce index refusal first; run Phemex/Binance focused native tests,
   touched fmt/Clippy and bans with CARGO_BUILD_JOBS=4. Independent review,
   PR into feat/rust-rewrite, green CI, revert guard, merge and report.

Phemex index triggers follow the official USDⓈ-M g-order schema:
https://phemex-docs.github.io/#place-order-http-put-prefered-2
`triggerType`, `tpTrigger`, and `slTrigger` accept `ByIndexPrice`. Readback
already preserves it; native placement now honors explicit SL/TP index sources.
TS silently falls back to mark/last for index sources, so this is an intentional
correction. Existing unspecified trigger defaults are preserved.

Daily candle reads keep the exact native caller cutoff. Read-only public API
verification returned the caller's current daily bar before a 22:13 UTC cutoff;
TS's previous-day shift loses that bar and another at the exclusive boundary.
A scripted regression pins both the request bound and retained timestamps.

The uncached cancellation guard remains: an ID alone cannot prove symbol and
position side. Account polling remains policy-controlled at 2 s; socket
positions/balances update immediately and accepted local writes refresh REST.
An extra terminal-event debounce would add currency reads and override deliberate
slow/paused polling for a small default latency reduction.

Verification: index-trigger regression reproduced the prior unsupported error.
All 400 focused Phemex/Binance native tests pass, as do touched fmt, strict
all-target Clippy and banned-pattern checks. Independent correctness review is
clear. Full TypeScript/WASM/replay validation remains CI's gate before merge.
- Integrated pinned34adfdb71a6 (including success-only Cx.done) via clean porcelain
  merge. Updated the added daily-candle success assertion to plain String;
  independent integration rereview clean. Phemex/Binance400 tests, fmt, strict
  all-target Clippy and bans pass again. Guard remains six intended files.
