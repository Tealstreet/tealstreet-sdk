# Plan and evidence index

Keep the evidence documents. This index routes readers to their topic, status and
read trigger rather than merging or deleting historical plans. Counts and old
progress entries inside a retained document describe their recorded revision;
use the current audit ledgers, source and actual merge pins to establish today's
behavior. “Retained plan/evidence” does not mean an open task or a parity pass.

The inventory covers every tracked path under `docs/plans` at this revision,
including historical JSON receipts and this index. Add a row when adding a plan.

| Topic | Status | Read trigger | File |
| --- | --- | --- | --- |
| CI Corepack Yarn preparation | Active infrastructure plan | Before changing CI package-manager bootstrap, retries or dependency-cache ordering. | [CI_COREPACK_YARN.md](CI_COREPACK_YARN.md) |
| Cumulative fill derivation | Active engine standardization plan | Before cumulative-to-incremental execution preparation, delivered progress or native fill dedup changes. | [RUST_CUMULATIVE_FILLS.md](RUST_CUMULATIVE_FILLS.md) |
| Row admission | Active engine standardization plan | Before keyed private REST/WS rejection, snapshot retention or coverage migration. | [RUST_ROW_ADMISSION.md](RUST_ROW_ADMISSION.md) |
| Public TS recording | Active harness plan | Before TS-origin public capture or extending audited transport/venue support; live capture remains owner evidence. | [HARNESS_TS_PUBLIC_RECORDER.md](HARNESS_TS_PUBLIC_RECORDER.md) |
| Account settings | Retained plan/evidence | Changing leverage, margin rails or settings refresh behavior. | [RUST_BINGX_LEVERAGE_RACES.md](RUST_BINGX_LEVERAGE_RACES.md) |
| Account settings | Retained plan/evidence | Changing leverage, margin rails or settings refresh behavior. | [RUST_BITUNIX_SETTINGS_READ_PROGRESS.md](RUST_BITUNIX_SETTINGS_READ_PROGRESS.md) |
| Account settings | Retained plan/evidence | Changing leverage, margin rails or settings refresh behavior. | [RUST_BYBIT_LEVERAGE_READ_FENCE.md](RUST_BYBIT_LEVERAGE_READ_FENCE.md) |
| Account settings | Retained plan/evidence | Changing leverage, margin rails or settings refresh behavior. | [RUST_LEVERAGE_SIDE_PROGRESS.md](RUST_LEVERAGE_SIDE_PROGRESS.md) |
| ApeX Omni native port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_APEXOMNI_PROGRESS.md](RUST_APEXOMNI_PROGRESS.md) |
| App stop payload audit and future implementation plan | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_APP_STOP_PAYLOADS.md](RUST_APP_STOP_PAYLOADS.md) |
| Architecture and cutover | Specification / historical port receipts | Engine architecture, frame contracts, port roster or cutover rules. | [RUST_REWRITE.md](RUST_REWRITE.md) |
| Architecture and gate evidence | Active re-audit | Current architecture cleanup, gate-proof boundaries and owner decisions. | [RUST_ARCHITECTURE_GATE_AUDIT.md](RUST_ARCHITECTURE_GATE_AUDIT.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [GRVT_REPLAY_PERF.md](GRVT_REPLAY_PERF.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [GRVT_RUST_PERF.md](GRVT_RUST_PERF.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [RUST_BYBIT_WASM_SIZE.md](RUST_BYBIT_WASM_SIZE.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [RUST_FAMILY_WASM_SIZE.md](RUST_FAMILY_WASM_SIZE.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [RUST_GATE_TIMING.md](RUST_GATE_TIMING.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [RUST_WASM_ARTIFACT_SIZE.md](RUST_WASM_ARTIFACT_SIZE.md) |
| Artifacts and performance | Retained plan/evidence | Changing artifact layout, compilation profiles, budgets or performance evidence. | [RUST_WASM_FAMILIES.md](RUST_WASM_FAMILIES.md) |
| AsterDEX Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_ASTERDEX.md](RUST_ASTERDEX.md) |
| Backpack Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BACKPACK.md](RUST_BACKPACK.md) |
| Binance / Bybit audit follow-up | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md](RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md) |
| Binance / Bybit trading audit fixes | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_AUDIT_BINANCE_BYBIT.md](RUST_AUDIT_BINANCE_BYBIT.md) |
| Binance variants port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [BINANCE_VARIANTS_PORT.md](BINANCE_VARIANTS_PORT.md) |
| BingX perpetual Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BINGX_PORT.md](RUST_BINGX_PORT.md) |
| Bitget Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITGET.md](RUST_BITGET.md) |
| Bitrue futures Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [BITRUE_RUST_PORT.md](BITRUE_RUST_PORT.md) |
| Bitunix Rust futures port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX.md](RUST_BITUNIX.md) |
| Bitunix account read recovery | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_ACCOUNT_READ_RECOVERY.md](RUST_BITUNIX_ACCOUNT_READ_RECOVERY.md) |
| Bitunix balance progress during private notifications | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_BALANCE_READ_PROGRESS.md](RUST_BITUNIX_BALANCE_READ_PROGRESS.md) |
| Bitunix order read scheduling | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_ORDER_READ_SCHEDULING.md](RUST_BITUNIX_ORDER_READ_SCHEDULING.md) |
| Bitunix ordinary order reconciliation under WebSocket traffic | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_KEYED_ORDER_READS.md](RUST_BITUNIX_KEYED_ORDER_READS.md) |
| Bitunix position reconciliation during continuous WS notifications | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_POSITION_READ_PROGRESS.md](RUST_BITUNIX_POSITION_READ_PROGRESS.md) |
| Bitunix secondary GET identity and egress | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BITUNIX_SECONDARY_READS.md](RUST_BITUNIX_SECONDARY_READS.md) |
| BloFin Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [BLOFIN_RUST_PORT.md](BLOFIN_RUST_PORT.md) |
| BreakoutProp Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_BREAKOUTPROP.md](RUST_BREAKOUTPROP.md) |
| Browser module readiness | Merged measurement contract | Chrome worker measurements and provenance; threshold proposal requires landing baselines. | [RUST_BROWSER_COLD_START.md](RUST_BROWSER_COLD_START.md) |
| Bybit linear port checkpoint | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [BYBIT_PORT.md](BYBIT_PORT.md) |
| Bybit variants port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [BYBIT_VARIANTS_PORT.md](BYBIT_VARIANTS_PORT.md) |
| Clock synchronization | Active standardisation | Changing engine event time, synchronized production clocks or injected replay clocks. | [RUST_ENGINE_CLOCK.md](RUST_ENGINE_CLOCK.md) |
| Coinbase Rust venue port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_COINBASE.md](RUST_COINBASE.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_BRIDGE_CAPABILITY_TAIL.md](RUST_BRIDGE_CAPABILITY_TAIL.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_CAPABILITIES.md](RUST_CAPABILITIES.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_PRODUCT_SEMANTICS.md](RUST_PRODUCT_SEMANTICS.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_STATIC_CONSTRUCTION_RULES.md](RUST_STATIC_CONSTRUCTION_RULES.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_VENUE_CONSTRUCTION.md](RUST_VENUE_CONSTRUCTION.md) |
| Construction and capabilities | Retained plan/evidence | Changing engine construction, option validation or capability-driven runtime behavior. | [RUST_VENUE_OPTIONS.md](RUST_VENUE_OPTIONS.md) |
| Credentialed evidence | Tooling / live evidence prerequisite | Planning approved live validation; scripted checks do not prove live private behavior. | [RUST_LIVE_GATE.md](RUST_LIVE_GATE.md) |
| Credentialed evidence | Tooling / live evidence prerequisite | Credentialed lifecycle recording, approval, environment fences and cleanup. | [RUST_LIVE_SMOKE.md](RUST_LIVE_SMOKE.md) |
| Credentials and attribution | Scoped production repair | Changing Hyperliquid fee-tier account/vault identity or role discovery. | [RUST_HYPERLIQUID_FEE_SUBJECT.md](RUST_HYPERLIQUID_FEE_SUBJECT.md) |
| Credentials and attribution | Retained plan/evidence | Changing credential selection, auth evidence, identity or broker routing. | [RUST_AUTH_RESULTS.md](RUST_AUTH_RESULTS.md) |
| Credentials and attribution | Retained plan/evidence | Changing credential selection, auth evidence, identity or broker routing. | [RUST_AUTH_RESULTS_SLICE_A.md](RUST_AUTH_RESULTS_SLICE_A.md) |
| Credentials and attribution | Retained plan/evidence | Changing credential selection, auth evidence, identity or broker routing. | [RUST_AUTH_RESULTS_SLICE_B.md](RUST_AUTH_RESULTS_SLICE_B.md) |
| Credentials and attribution | Retained plan/evidence | Changing credential selection, auth evidence, identity or broker routing. | [RUST_BROKER_ATTRIBUTION.md](RUST_BROKER_ATTRIBUTION.md) |
| Credentials and attribution | Retained plan/evidence | Changing credential selection, auth evidence, identity or broker routing. | [RUST_IDENTITY_ROUTING.md](RUST_IDENTITY_ROUTING.md) |
| Crypto.com Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [venue-ports/CRYPTOCOM.md](venue-ports/CRYPTOCOM.md) |
| Cutover tail | Scoped evidence ledger | Checking SDK-owned cutover repairs versus remaining caller-owned work. | [RUST_CUTOVER_TAIL_AUDIT.md](RUST_CUTOVER_TAIL_AUDIT.md) |
| Downstream contracts | Current caller obligations | SDK/NX caller migration, deployment ordering and deletion prerequisites. | [RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md) |
| Execution and command state | Retained plan/evidence | Changing fill attribution, command outcomes or uncertain-order lifecycle. | [RUST_COMMAND_IDENTITIES.md](RUST_COMMAND_IDENTITIES.md) |
| Execution and command state | Retained plan/evidence | Changing fill attribution, command outcomes or uncertain-order lifecycle. | [RUST_FILL_EXECUTION_ID.md](RUST_FILL_EXECUTION_ID.md) |
| Execution and command state | Retained plan/evidence | Changing signed dead-man controls, UTC deadlines or uncertain-write ownership. | [RUST_HYPERLIQUID_SCHEDULE_CANCEL.md](RUST_HYPERLIQUID_SCHEDULE_CANCEL.md) |
| Execution and command state | Retained plan/evidence | Changing fill attribution, command outcomes or uncertain-order lifecycle. | [RUST_ORDER_RACES.md](RUST_ORDER_RACES.md) |
| Execution and command state | Retained plan/evidence | Changing fill attribution, command outcomes or uncertain-order lifecycle. | [RUST_UNKNOWN_CANCEL.md](RUST_UNKNOWN_CANCEL.md) |
| Execution and command state | Retained plan/evidence | Changing fill attribution, command outcomes or uncertain-order lifecycle. | [RUST_WEEX_EMPTY_FILL_IDENTITY.md](RUST_WEEX_EMPTY_FILL_IDENTITY.md) |
| GRVT perpetuals Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [GRVT_RUST_PORT.md](GRVT_RUST_PORT.md) |
| Gate integrity | Active harness plan | Orderly mutation ACK staging and final-snapshot causality across brokers/networks. | [HARNESS_ORDERLY_ACK_FENCES.md](HARNESS_ORDERLY_ACK_FENCES.md) |
| Gate integrity | Active harness implementation | Hyperliquid L1 request authentication, generated-ID intent binding and matched-ACK validation. | [HARNESS_HYPERLIQUID_REQUEST_VALIDATION.md](HARNESS_HYPERLIQUID_REQUEST_VALIDATION.md) |
| Gate integrity | Retained implementation evidence | Understanding fixer isolation, strict comparison and verifier design. | [RUST_GATE_HONESTY.md](RUST_GATE_HONESTY.md) |
| Gate integrity | Historical result ledger | Tracing original known FAIL classifications; use fresh reports for current results. | [RUST_GATE_HONESTY_RESULTS.md](RUST_GATE_HONESTY_RESULTS.md) |
| Historical gate receipts | Retained JSON evidence | Checking the named historical gate set; not a current CI certificate. | [venue-ports/KUCOIN_GATES.json](venue-ports/KUCOIN_GATES.json) |
| Historical gate receipts | Retained JSON evidence | Checking the named historical gate set; not a current CI certificate. | [venue-ports/PARITY_BYBIT_CI_GATES.json](venue-ports/PARITY_BYBIT_CI_GATES.json) |
| Historical gate receipts | Retained JSON evidence | Checking the named historical gate set; not a current CI certificate. | [venue-ports/PARITY_GATES.json](venue-ports/PARITY_GATES.json) |
| Historical gate receipts | Retained JSON evidence | Checking the named historical gate set; not a current CI certificate. | [venue-ports/PARITY_PUBLIC_ONLY_FIX_GATES.json](venue-ports/PARITY_PUBLIC_ONLY_FIX_GATES.json) |
| Historical gate receipts | Retained JSON evidence | Checking the named historical gate set; not a current CI certificate. | [venue-ports/SOCKET_SUPERVISOR_GATES.json](venue-ports/SOCKET_SUPERVISOR_GATES.json) |
| History completion and pagination | Active core plan | Shared command-owned completeness, pager fan-out and capability conformance follow-up. | [RUST_OWNED_HISTORY.md](RUST_OWNED_HISTORY.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_BITUNIX_HISTORY_COMPLETENESS.md](RUST_BITUNIX_HISTORY_COMPLETENESS.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_BLOFIN_HISTORY_MARKER_OWNER.md](RUST_BLOFIN_HISTORY_MARKER_OWNER.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_BYBIT_HISTORY_MARKER_OWNER.md](RUST_BYBIT_HISTORY_MARKER_OWNER.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_HISTORY_COMPLETENESS_TRANSPORT.md](RUST_HISTORY_COMPLETENESS_TRANSPORT.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_HISTORY_PAGING.md](RUST_HISTORY_PAGING.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_HYPERLIQUID_HISTORY_MARKER_OWNER.md](RUST_HYPERLIQUID_HISTORY_MARKER_OWNER.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_OKX_HISTORY_MARKER_OWNER.md](RUST_OKX_HISTORY_MARKER_OWNER.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_PARADEX_HISTORY_MARKER_OWNER.md](RUST_PARADEX_HISTORY_MARKER_OWNER.md) |
| History and completeness | Retained plan/evidence | Changing history paging, request-owned incomplete results or transport propagation. | [RUST_WEEX_HISTORY_MARKER_OWNER.md](RUST_WEEX_HISTORY_MARKER_OWNER.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_BRIDGE_FIXES.md](RUST_BRIDGE_FIXES.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_FRAME_SERVER_PROGRESS.md](RUST_FRAME_SERVER_PROGRESS.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_MOBILE_B3.md](RUST_MOBILE_B3.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_SERVER_ADAPTER_PROGRESS.md](RUST_SERVER_ADAPTER_PROGRESS.md) |
| Hosts and SDK transports | Retained plan/evidence | Changing host/frame contracts, SDK forwarding or platform integration. | [RUST_TRANSPORT.md](RUST_TRANSPORT.md) |
| Hyperliquid Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [HYPERLIQUID_RUST_PORT.md](HYPERLIQUID_RUST_PORT.md) |
| Hyperliquid and Kraken spot products | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_SPOT_PRODUCTS.md](RUST_SPOT_PRODUCTS.md) |
| Kraken futures Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [KRAKEN_RUST_PORT.md](KRAKEN_RUST_PORT.md) |
| KuCoin Spot parity | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [venue-ports/KUCOIN_SPOT.md](venue-ports/KUCOIN_SPOT.md) |
| KuCoin futures Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [venue-ports/KUCOIN.md](venue-ports/KUCOIN.md) |
| Lighter Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [LIGHTER_RUST_PORT.md](LIGHTER_RUST_PORT.md) |
| Lighter deployment and signing variants | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [LIGHTER_VARIANTS_PORT.md](LIGHTER_VARIANTS_PORT.md) |
| Master merge blockers | Open cutover checklist | Before Sam approves the single landing → master merge. | [RUST_MASTER_MERGE_BLOCKERS.md](RUST_MASTER_MERGE_BLOCKERS.md) |
| MEXC futures port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_MEXC_PROGRESS.md](RUST_MEXC_PROGRESS.md) |
| MEXC, Bitrue and BloFin audit repairs | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_VENUE_HIGHS_A_PROGRESS.md](RUST_VENUE_HIGHS_A_PROGRESS.md) |
| Navigation | Current index | Finding the relevant plan, contract or historical evidence. | [README.md](README.md) |
| OKX Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_OKX.md](RUST_OKX.md) |
| Orderly family Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_ORDERLY.md](RUST_ORDERLY.md) |
| Orderly port-review lows | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_ORDERLY_LOWS.md](RUST_ORDERLY_LOWS.md) |
| Paradex and exchange listener audit repairs | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_PARADEX_EMITTER.md](RUST_PARADEX_EMITTER.md) |
| Paradex perpetuals Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_PARADEX.md](RUST_PARADEX.md) |
| Phase 9 consumer blockers | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_CONSUMER_BLOCKERS.md](RUST_CONSUMER_BLOCKERS.md) |
| Phase 9: consumer cutover readiness | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_CUTOVER_READINESS.md](RUST_CUTOVER_READINESS.md) |
| Phemex perpetuals Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_PHEMEX.md](RUST_PHEMEX.md) |
| Polymarket CLOB V2 Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [venue-ports/POLYMARKET.md](venue-ports/POLYMARKET.md) |
| Product scope | Recorded owner decision | GRVT perpetual-only scope and retired mapper-only products. | [GRVT_PRODUCTS_DECISION.md](GRVT_PRODUCTS_DECISION.md) |
| Production findings | Current audit ledger | Choosing remaining work or deciding whether a historical finding is still open. | [RUST_AUDITS.md](RUST_AUDITS.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_BACKPACK_REST_ORDERBOOK.md](RUST_BACKPACK_REST_ORDERBOOK.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_CRYPTOCOM_REST_ORDERBOOK.md](RUST_CRYPTOCOM_REST_ORDERBOOK.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_GRVT_REST_ORDERBOOK.md](RUST_GRVT_REST_ORDERBOOK.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_MEXC_BOOK_JSON_INGRESS.md](RUST_MEXC_BOOK_JSON_INGRESS.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_PARADEX_REST_ORDERBOOK.md](RUST_PARADEX_REST_ORDERBOOK.md) |
| REST order-book queries | Retained plan/evidence | Changing request-owned book snapshots, JSON ingress or streaming isolation. | [RUST_WEEX_REST_ORDERBOOK.md](RUST_WEEX_REST_ORDERBOOK.md) |
| Replay determinism | Sweep plan / merge receipts | Private clock, fixture causality and complete raw repeatability proofs. | [HARNESS_TAPE_TIME.md](HARNESS_TAPE_TIME.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [HARNESS_ASTER_CLOCK.md](HARNESS_ASTER_CLOCK.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_BITGET_PARITY.md](RUST_BITGET_PARITY.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_MEXC_PARITY.md](RUST_MEXC_PARITY.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_GAPS.md](RUST_PARITY_GAPS.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_SWEEP.md](RUST_PARITY_SWEEP.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_SWEEP_AUDIT.md](RUST_PARITY_SWEEP_AUDIT.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_SWEEP_FINAL.md](RUST_PARITY_SWEEP_FINAL.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_SWEEP_GATES.md](RUST_PARITY_SWEEP_GATES.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PARITY_SWEEP_PRODUCT_UNION.md](RUST_PARITY_SWEEP_PRODUCT_UNION.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PORT_WIRE_SWEEP.md](RUST_PORT_WIRE_SWEEP.md) |
| Replay evidence | Retained plan/evidence | Checking strict reports, fixture provenance, declarations or repeatability proofs. | [RUST_PORT_WIRE_SWEEP_RECEIPTS.md](RUST_PORT_WIRE_SWEEP_RECEIPTS.md) |
| Repository policy | Process evidence | Branch-history secret scanning and exact fixture exemptions. | [BRANCH_SECRET_SCAN.md](BRANCH_SECRET_SCAN.md) |
| Restore KuCoin Spot family throughput profile | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_KUCOIN_SPOT_THROUGHPUT.md](RUST_KUCOIN_SPOT_THROUGHPUT.md) |
| Rollback and cutover | Recommendation / owner decision pending | Choosing engine selection, TS reachability, rollback or soak/deletion policy. | [RUST_ROLLBACK.md](RUST_ROLLBACK.md) |
| Security and signing | Retained plan/evidence | Changing credential handling, entropy, signing or transport security. | [RUST_APEXOMNI_SIGNING.md](RUST_APEXOMNI_SIGNING.md) |
| Security and signing | Retained plan/evidence | Changing credential handling, entropy, signing or transport security. | [RUST_HOST_ENTROPY.md](RUST_HOST_ENTROPY.md) |
| Security and signing | Retained plan/evidence | Changing credential handling, entropy, signing or transport security. | [RUST_SECURITY_FIXES.md](RUST_SECURITY_FIXES.md) |
| Shared engine standards | Current standards / audit ledger | Error taxonomy, coded notices, helpers and capability ownership. | [RUST_QUALITY.md](RUST_QUALITY.md) |
| Snapshot isolation | Retained plan/evidence | Changing row mapping, retention, freshness fences or atomic snapshot publication. | [RUST_BITGET_WS_ROW_STARVATION.md](RUST_BITGET_WS_ROW_STARVATION.md) |
| Snapshot isolation | Retained plan/evidence | Changing row mapping, retention, freshness fences or atomic snapshot publication. | [RUST_BITUNIX_ORDER_ROW_ISOLATION.md](RUST_BITUNIX_ORDER_ROW_ISOLATION.md) |
| Snapshot isolation | Retained plan/evidence | Changing row mapping, retention, freshness fences or atomic snapshot publication. | [RUST_BITUNIX_POSITION_ROW_ISOLATION.md](RUST_BITUNIX_POSITION_ROW_ISOLATION.md) |
| Snapshot isolation | Retained plan/evidence | Changing row mapping, retention, freshness fences or atomic snapshot publication. | [RUST_BYBIT_ROW_STARVATION.md](RUST_BYBIT_ROW_STARVATION.md) |
| Snapshot isolation | Retained plan/evidence | Changing row mapping, retention, freshness fences or atomic snapshot publication. | [RUST_PRIVATE_ROWS_TICKERS.md](RUST_PRIVATE_ROWS_TICKERS.md) |
| Sockets and host routing | Retained plan/evidence | Changing connection supervision, authentication, recovery, routing or clocks. | [RUST_BITUNIX_FORCED_WS_ROUTE.md](RUST_BITUNIX_FORCED_WS_ROUTE.md) |
| Sockets and host routing | Retained plan/evidence | Changing connection supervision, authentication, recovery, routing or clocks. | [RUST_BITUNIX_HOST_CLOCK.md](RUST_BITUNIX_HOST_CLOCK.md) |
| Sockets and host routing | Retained plan/evidence | Changing connection supervision, authentication, recovery, routing or clocks. | [RUST_BITUNIX_LOGIN_CONTROLS.md](RUST_BITUNIX_LOGIN_CONTROLS.md) |
| Sockets and host routing | Retained plan/evidence | Changing connection supervision, authentication, recovery, routing or clocks. | [RUST_BITUNIX_SOCKET_LIFECYCLE.md](RUST_BITUNIX_SOCKET_LIFECYCLE.md) |
| Sockets and host routing | Retained plan/evidence | Changing connection supervision, authentication, recovery, routing or clocks. | [RUST_SOCKET_SUPERVISOR.md](RUST_SOCKET_SUPERVISOR.md) |
| TxFlow Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [TXFLOW_RUST_PORT.md](TXFLOW_RUST_PORT.md) |
| Venue findings lane C | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_VENUES_C.md](RUST_VENUES_C.md) |
| Venue implementation | Contributor contract | Before starting a venue port or a venue behavior change. | [VENUE_PORT_BRIEF.md](VENUE_PORT_BRIEF.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_BINGX_PARTIAL_ERRORS.md](RUST_BINGX_PARTIAL_ERRORS.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_BITUNIX_PUBLIC_STARTUP.md](RUST_BITUNIX_PUBLIC_STARTUP.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_CODED_LOGGING.md](RUST_CODED_LOGGING.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_ERROR_HANDLING.md](RUST_ERROR_HANDLING.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_GRVT_BUILDER_ERROR.md](RUST_GRVT_BUILDER_ERROR.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_LOG_SHIM_REMOVAL.md](RUST_LOG_SHIM_REMOVAL.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_PUBLIC_STARTUP.md](RUST_PUBLIC_STARTUP.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_PUBLIC_STARTUP_EXTRA_EIGHT.md](RUST_PUBLIC_STARTUP_EXTRA_EIGHT.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_RESILIENCE.md](RUST_RESILIENCE.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_STARTUP_AUDIT.md](RUST_STARTUP_AUDIT.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_STARTUP_BROKER.md](RUST_STARTUP_BROKER.md) |
| Visibility and recovery | Retained plan/evidence | Changing error visibility, coded notices, startup or retry/recovery policy. | [RUST_STARTUP_RETRY.md](RUST_STARTUP_RETRY.md) |
| WEEX Contract V3 Rust lane | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_WEEX.md](RUST_WEEX.md) |
| WEEX selected-symbol metadata refresh | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_WEEX_METADATA_REFRESH.md](RUST_WEEX_METADATA_REFRESH.md) |
| WOO perpetual Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_WOO_PROGRESS.md](RUST_WOO_PROGRESS.md) |
