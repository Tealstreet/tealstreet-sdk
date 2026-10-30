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
| BloFin managed public/private sockets | Plan and production-binding originals; one venue PR | Before changing authenticated login custody, public interest recovery or full order-census epoch | [RUST_BLOFIN_MANAGED_SOCKETS.md](RUST_BLOFIN_MANAGED_SOCKETS.md) |
| Legacy to stable-subject nonce handoff | Engine-only plan GO; no consumer activation | Before transferring an owned legacy nonce issuer into managed NonceClaim custody. | [RUST_NONCE_DOMAIN_HANDOFF.md](RUST_NONCE_DOMAIN_HANDOFF.md) |
| Address-keyed dispatch nonce domains | Plan directions CLEAR; local originals checkpoint | Before extending #2093 claims to verified Aster V3 signer and Hyperliquid agent subjects; no non-adopter growth or consumer activation implied | [RUST_ADDRESS_NONCE_DOMAINS.md](RUST_ADDRESS_NONCE_DOMAINS.md) |
| Per-venue engine feature gating | Plan; Phase 1 managed-sockets, Phase 2 barriers; no source | Before adding a per-venue engine feature, or changing how Engine<V> reaches managed-socket or barrier code. | [RUST_ENGINE_FEATURE_GATING.md](RUST_ENGINE_FEATURE_GATING.md) |
| WEEX depth and socket custody | Plan/native/API CLEAR; first19 originals13RED/6GREEN local; source held | Remaining Depth observations and both native socket lifecycles; truthful inventory residue disposition and fixed size gates | [RUST_WEEX_DEPTH_SOCKET_CONTEXT.md](RUST_WEEX_DEPTH_SOCKET_CONTEXT.md) |
| Request macro provenance precision | Plan only; overseer-approved WEEX false-positive correction | Before pruning pure data macro rows while preserving raw sender and cross-venue gate coverage | [RUST_REQUEST_MACRO_PROVENANCE.md](RUST_REQUEST_MACRO_PROVENANCE.md) |
| Shared repair Mutation authority | Joint plan; engine direction CLEAR, originals/source separate | Before Phemex late-cancel or Backpack trigger-repair migration, snapshot-origin authority, subject lifecycle or joint cancel batching. | [RUST_REPAIR_MUTATION_AUTHORITY.md](RUST_REPAIR_MUTATION_AUTHORITY.md) |
| Bybit managed-socket adoption | Plan, originals and source in one PR after #2161 | Before changing Bybit socket feeds, its capability declaration or its 1,445,888 B budget. | [RUST_BYBIT_SOCKET_ADOPTION.md](RUST_BYBIT_SOCKET_ADOPTION.md) |
| AsterDEX managed-socket adoption | Plan, originals and source in one PR (interim listen-key shape B by ruling) | Before changing AsterDEX socket feeds, its listen-key acquisition or its Phase C migration. | [RUST_ASTERDEX_SOCKET_ADOPTION.md](RUST_ASTERDEX_SOCKET_ADOPTION.md) |
| Managed socket core size | Step 1 (handoff capability) plan amendment and originals; source after review | Before changing socket capability reachability, Pending storage or Driver scheduling to reduce adoption size. | [RUST_MANAGED_SOCKET_CORE_SHRINK.md](RUST_MANAGED_SOCKET_CORE_SHRINK.md) |
| Paradex command candle Read custody | Local source; scoped controls pass, source review pending | Before changing command candle fallback, caller deadlines or its two terminal inventory prunes. | [RUST_PARADEX_CANDLE_COMMAND_CONTEXT.md](RUST_PARADEX_CANDLE_COMMAND_CONTEXT.md) |
| Request-context inventory precision | Implementation and rebaseline; review pending | Before changing `check-request-context.py`, its manifests, or what counts as a legacy `Cx` call. | [RUST_REQUEST_CONTEXT_PRECISION.md](RUST_REQUEST_CONTEXT_PRECISION.md) |
| BreakoutProp options refusal (unreachable arm deleted) | One-site prune; engine pre-entry refusal pinned by a control | Before changing BreakoutProp options handling or its capability flags. | [RUST_BREAKOUTPROP_OPTIONS_LOCAL_CONTEXT.md](RUST_BREAKOUTPROP_OPTIONS_LOCAL_CONTEXT.md) |
| BingX local candle refusal custody | Draft conversion; source review/CI pending | Before replacing the three synchronous candle refusals; remote/key-selection paths excluded. | [RUST_BINGX_LOCAL_CANDLE_CONTEXT.md](RUST_BINGX_LOCAL_CANDLE_CONTEXT.md) |
| WEEX public discovery and ticker-seed custody | Source/native review CLEAR; local checks and paired/current sizes PASS; frozen-head CI pending | Before managing Markets, Daily and BookTickers or changing their publication/retirement order. | [RUST_WEEX_PUBLIC_SEED_CONTEXT.md](RUST_WEEX_PUBLIC_SEED_CONTEXT.md) |
| WEEX candle query Read custody | Draft source; scoped controls pass | Before managing candle HTTP/completions or interpreting caller deadline and streamed failure changes. | [RUST_WEEX_CANDLE_CONTEXT.md](RUST_WEEX_CANDLE_CONTEXT.md) |
| WEEX local leverage refusal custody | Draft conversion; source review/CI pending | Before replacing three synchronous FetchLeverage refusals; selection and signed read remain unchanged. | [RUST_WEEX_LOCAL_LEVERAGE_CONTEXT.md](RUST_WEEX_LOCAL_LEVERAGE_CONTEXT.md) |
| WEEX leverage initial managed admission | Draft implementation; source review pending | Before replacing the one numeric setup fallback; key choice and physical bound are unchanged. | [RUST_WEEX_LEVERAGE_ADMISSION_CONTEXT.md](RUST_WEEX_LEVERAGE_ADMISSION_CONTEXT.md) |
| BingX trading and request funnel | Plan; supersedes #1954 (adopted per overseer ruling) | Before moving BingX ordered writes to the streamed serial Plan, independent cancel batches, or the keyless read and legacy stage() funnel. | [RUST_BINGX_REQUEST_FUNNEL.md](RUST_BINGX_REQUEST_FUNNEL.md) |
| BingX history paging custody | Implementation under native/source review | Before moving native fill/range pages and signature resends onto one caller-wait Read root. | [RUST_BINGX_HISTORY_CONTEXT.md](RUST_BINGX_HISTORY_CONTEXT.md) |
| WEEX public depth-query custody | Original assertions reviewed; production conversion | Before managing command-owned unsigned order-book snapshots and their four numeric terminals. | [RUST_WEEX_DEPTH_QUERY_CONTEXT.md](RUST_WEEX_DEPTH_QUERY_CONTEXT.md) |
| Polymarket public read custody | Draft adoption; local checks pass | Before adopting public books/candles and local completion or pruning their inventory. | [RUST_POLYMARKET_PUBLIC_READ_CONTEXT.md](RUST_POLYMARKET_PUBLIC_READ_CONTEXT.md) |
| Kraken whole-family socket lifecycle | Plan; native/API review pending | Before adopting all four Futures/Spot feeds, pruning nine socket references or changing challenge/token retirement. | [RUST_KRAKEN_SOCKET_LIFECYCLE.md](RUST_KRAKEN_SOCKET_LIFECYCLE.md) |
| Kraken public discovery and local custody | Draft adoption; local checks and independent reviews clear | Before managing unsigned discovery reads or local command terminals in Futures/Spot. | [RUST_KRAKEN_PUBLIC_LOCAL_CONTEXT.md](RUST_KRAKEN_PUBLIC_LOCAL_CONTEXT.md) |

| WEEX caller-root history paging | Source checkpoint; actual #1983 integrated | Before converting WEEX list/income pages, paced windows or their history completion. | [RUST_WEEX_HISTORY_CONTEXT.md](RUST_WEEX_HISTORY_CONTEXT.md) |
| Subscription handoff and native read continuation | Shared ordering correction | Before issuing native reads after managed subscription frames or changing final drain/timer custody. | [RUST_SOCKET_INTEREST_HANDOFF.md](RUST_SOCKET_INTEREST_HANDOFF.md) |
| Hyperliquid whole-family managed sockets | Plan and original-source controls | Before moving the multiplexed perps/spot public and private socket lifecycle or changing subscription-to-refresh ordering. | [RUST_HYPERLIQUID_MANAGED_SOCKETS.md](RUST_HYPERLIQUID_MANAGED_SOCKETS.md) |
| Caller command wait in the engine | Plan under review (claude-fe7lgm) | Before bounding engine work by the caller's `rustCommandTimeoutMs` or reading `Cx::command_wait()`. | [RUST_COMMAND_TIMEOUT.md](RUST_COMMAND_TIMEOUT.md) |
| WOO public steady CPU headroom | Implemented; public CI 1.78x | Before optimizing WOO public steady CPU or interpreting its named-WASM profile and local medians. | [RUST_WOO_PUBLIC_CPU.md](RUST_WOO_PUBLIC_CPU.md) |
| Conditional mutation completion | Shared post-#1873 implementation plan | Before retiring an unissued mutation suffix after an exact declared terminal delivery. | [RUST_PLAN_CONDITIONAL_TERMINAL.md](RUST_PLAN_CONDITIONAL_TERMINAL.md) |
| Retained Preflight readiness | Shared runtime implemented; parent/CI gates pending | Before activating a queued command outside its own initial callback or read handoff. | [RUST_PREFLIGHT_READY.md](RUST_PREFLIGHT_READY.md) |
| AsterDEX RequestContext adoption | Pinned inventory and single-request-first plan | Before migrating Aster HTTP/completion, pruning its legacy inventory or touching files claimed by concurrent PRs. | [RUST_ASTERDEX_REQUEST_CONTEXT.md](RUST_ASTERDEX_REQUEST_CONTEXT.md) |
| Phemex RequestContext adoption | Pinned inventory and adoption plan | Before Phemex HTTP/completion, repair mutations, signer projection or legacy inventory changes. | [RUST_PHEMEX_REQUEST_CONTEXT.md](RUST_PHEMEX_REQUEST_CONTEXT.md) |
| Phemex bounded REST history custody | Plan and original RED phase | Before migrating the one-response fills/orders/trades/lookup paths or pruning their six legacy fingerprints. | [RUST_PHEMEX_BOUNDED_HISTORY_CONTEXT.md](RUST_PHEMEX_BOUNDED_HISTORY_CONTEXT.md) |
| MEXC RequestContext adoption | Pinned inventory and adoption plan | Before MEXC managed reads/writes, startup refusal repair or legacy inventory changes. | [RUST_MEXC_REQUEST_CONTEXT.md](RUST_MEXC_REQUEST_CONTEXT.md) |
| WOO RequestContext adoption | Pinned inventory and adoption plan | Before WOO managed reads, routing, release pacing or legacy inventory changes. | [RUST_WOO_REQUEST_CONTEXT.md](RUST_WOO_REQUEST_CONTEXT.md) |
| WOO local history custody | Draft adoption; local gates pass, CI pending | Before changing synchronous WOO history refusals or private/public continuation ownership. | [RUST_WOO_LOCAL_HISTORY_CONTEXT.md](RUST_WOO_LOCAL_HISTORY_CONTEXT.md) |
| Crypto.com RequestContext adoption | Pinned inventory and public-depth implementation plan | Before Crypto.com standalone depth, signed POST read/write ownership or legacy inventory changes. | [RUST_CRYPTOCOM_REQUEST_CONTEXT.md](RUST_CRYPTOCOM_REQUEST_CONTEXT.md) |
| OKX-only socket-core variant | Option 1 preparation; no merge | Before selecting the OKX-only managed-sockets boundary or its CI evidence. | [RUST_SOCKET_OKX_ONLY_VARIANT.md](RUST_SOCKET_OKX_ONLY_VARIANT.md) |
| Public managed HTTP headers | Shared additive API plan | Before unsigned HL/GRVT JSON POSTs need static headers on the existing managed transport path. | [RUST_PUBLIC_HTTP_HEADERS.md](RUST_PUBLIC_HTTP_HEADERS.md) |
| Selected signer mode | Active shared SDK fix; queued after #1741 | Before changing RemoteSigner precedence, PayloadSigner admission or runtime feed selection with both callbacks. | [RUST_SELECTED_SIGNER_MODE.md](RUST_SELECTED_SIGNER_MODE.md) |
| Engine resource IDs | Planned shared allocator/host correction | Before changing physical socket/request/timer identities or managed host numeric ingress. | [RUST_SAFE_ENGINE_IDS.md](RUST_SAFE_ENGINE_IDS.md) |
| Hosted signer presentation | Planned after #1757 | Extending primitive signature encoding or provider-owned passphrase presentation for OKX/Bitget. | [RUST_HOSTED_SIGNER_PRESENTATION.md](RUST_HOSTED_SIGNER_PRESENTATION.md) |
| Gate failure ordering | Implemented shared harness fix; CI pending | Before changing gate failure serialization, console truncation or deterministic evidence comparisons. | [HARNESS_GATE_FAILURE_ORDER.md](HARNESS_GATE_FAILURE_ORDER.md) |
| AsterDEX signer admission | Plan; review pending | Before changing Aster credentials, signer setup, registry admission or remote/hosted signing for Aster or ApeX. | [RUST_ASTER_SIGNER_ADMISSION.md](RUST_ASTER_SIGNER_ADMISSION.md) |
| ApeX Omni signer admission | Plan; review pending | Before changing ApeX credentials, signer setup, registry admission, zkLink seed custody or ApeX signed REST/WS login. | [RUST_APEXOMNI_SIGNER_ADMISSION.md](RUST_APEXOMNI_SIGNER_ADMISSION.md) |
| ApeX Omni managed signed reads | Plan and original REDs; production waits on the ApeX unit landing | Before changing ApeX symbols, account snapshot, balance, identity or fill-history reads, or the lib.rs signed-read wrapper. | [RUST_APEXOMNI_READ_CONTEXT.md](RUST_APEXOMNI_READ_CONTEXT.md) |
| Backpack signer admission | Option B approved; implemented | Before changing Backpack registry identity, signer setup or the custody of Backpack key material. | [RUST_BACKPACK_SIGNER_ADMISSION.md](RUST_BACKPACK_SIGNER_ADMISSION.md) |
| Selected signed reads | Plan; draft PR | Before adding or changing a venue's managed signed-read adapter, read-key slot choice or Read policy wrapper (BingX, WEEX, OKX, Bitget, KuCoin). | [RUST_SELECTED_SIGNED_READ.md](RUST_SELECTED_SIGNED_READ.md) |
| Lighter Schnorr signer recipe | Part 1 implemented (#1967); signer-owner review clear | Before changing Lighter signing, the SchnorrDigest contract, or Lighter public-key or remote recipes. | [RUST_LIGHTER_SCHNORR_RECIPE.md](RUST_LIGHTER_SCHNORR_RECIPE.md) |
| Backpack public-only read provider | Plan and REDs, local; stacked on #2078 and #2073 | Before changing what Backpack installs in public-only mode, or how its history signs with a saved read key. | [RUST_BACKPACK_PUBLIC_READ_PROVIDER.md](RUST_BACKPACK_PUBLIC_READ_PROVIDER.md) |
| Session-fenced venue Bearer | Phase 1 and phase 2 parts 1-2 implemented and CLEAR; header bundle CLEAR; phase 3 (NotReady park, header order), phase 4 (post-6b refresh) and phase 5 (session-bearer feature gate) implemented and CLEAR; draft stacked on #2037 | Before attaching a venue session token (Bearer/JWT) to managed requests, or fencing requests to a Session generation. | [RUST_SESSION_BEARER_HOOK.md](RUST_SESSION_BEARER_HOOK.md) |
| Declared read-key policy | Plan; REDs next; consumers Crypto.com and Backpack | Before ignoring, validating-only or public-mode-installing a venue's read key, or changing how read-key material reaches the provider. | [RUST_READ_KEY_POLICY.md](RUST_READ_KEY_POLICY.md) |
| Credential selection and nonce authority | Active shared-core plan; review pending | Before choosing a credential slot, read key, account subject, auth evidence tag or signing nonce in venue code. | [RUST_CREDENTIAL_SELECTION.md](RUST_CREDENTIAL_SELECTION.md) |
| Ordinary Bybit Plan adoption | Plan + original REDs; child of #1873 | Before migrating ordinary Bybit trading onto Preflight/Plan, its code table, ACK proofs or resend paths. | [RUST_BYBIT_PLAN_ADOPTION.md](RUST_BYBIT_PLAN_ADOPTION.md) |
| Bybit options adoption | Plan + original REDs; slice 4 | Before moving Bybit option trading, option queries or option seeds, or deleting the legacy trading executor. | [RUST_BYBIT_OPTIONS.md](RUST_BYBIT_OPTIONS.md) |
| Credential-only trusted setup | Revision 4 landed as #2149 (G3 typed `PrimaryLogin` seam, G6 per-operation Optional `SigningNotConfigured`, no latch); phase 2 landed as #2132; phase 3 in review: D3 presence inside the setup material type plus `SetupFacts`, and D6 | Before installing a key-only identity (no signing ref), reading key presence (Absent/Empty/Blank/Nonblank) or changing `Identity.signing`. | [RUST_CREDENTIAL_ONLY_SETUP.md](RUST_CREDENTIAL_ONLY_SETUP.md) |
| Error classification | Active engine standardization plan; inventory ranked | Before classifying venue errors, native code tables, status-to-outcome rules, partial batch failures or error-driven resends. | [RUST_ERROR_CLASSIFICATION.md](RUST_ERROR_CLASSIFICATION.md) |
| HTTP command entry | Planned shared pre-authority rejection seam | Before removing numeric failure when initial Context issuance can refuse. | [RUST_HTTP_COMMAND_ENTRY.md](RUST_HTTP_COMMAND_ENTRY.md) |
| Background read delivery | Additive shared delivery guard plan | Before publishing a one-leg Background Read response or treating logical liveness as callback identity. | [RUST_BACKGROUND_READ_DELIVERY.md](RUST_BACKGROUND_READ_DELIVERY.md) |
| Request lifecycle and command outcomes | Active shared-core plan | Before HTTP retry/outcome sealing, actual-release admission, expiry projection or send pacing. | [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md) |
| Background-session Context | Plan; core not started | Before listen-key, WS-token or session-login HTTP, Background+Mutation contexts or session retry/fencing. | [RUST_BACKGROUND_SESSION.md](RUST_BACKGROUND_SESSION.md) |
| Command replacement groups | Local follow-on; publish after #1741 | Before batch-to-single supersession, ordinary Bybit outcome aggregation or managed native retries. | [RUST_COMMAND_REPLACEMENTS.md](RUST_COMMAND_REPLACEMENTS.md) |
| Shared execution groups | Inventory and settled API under review (claude-fe7lgm) | Before any joined-completion site (Hyro, metadata, Bitrue FetchLeverage, Bybit RefreshMarkets) or observer authority. | [RUST_EXECUTION_GROUPS.md](RUST_EXECUTION_GROUPS.md) |
| BingX signing | Active native checkpoint | Before BingX HTTP signing, typed parameter recipes, credential setup or signature resend changes. | [RUST_BINGX_DISPATCH_SIGNER.md](RUST_BINGX_DISPATCH_SIGNER.md) |
| Bitget signing | Active native checkpoint; hosted prerequisite pending | Before Bitget credential setup, private HTTP/WS builders or hosted signer adoption. | [RUST_BITGET_DISPATCH_SIGNER.md](RUST_BITGET_DISPATCH_SIGNER.md) |
| Binance history under shared Read authority | Plan only; no engine features; size measured, REDs next after review | Before converting Binance fills/orders/income/public-trades history or client-order lookup off the legacy request funnel. | [RUST_BINANCE_HISTORY_CONTEXT.md](RUST_BINANCE_HISTORY_CONTEXT.md) |
| Bitget caller adoption (candles, identity, account reads, trade entry) | Slice 1 implemented; history waits on #1945 | Before converting Bitget candles, FetchAccount, account-cycle reads, trade entry refusals or history to shared Context authority. | [RUST_BITGET_CALLER_ADOPTION.md](RUST_BITGET_CALLER_ADOPTION.md) |
| Bitget discovery under Background Context (slice 2) | Plan and original REDs | Before converting Bitget contracts/tickers discovery off the legacy `get`. | [RUST_BITGET_DISCOVERY_CONTEXT.md](RUST_BITGET_DISCOVERY_CONTEXT.md) |
| Bitget managed sockets and private-WS route parity | Plan; legacy-path seam next after dazzy3 CLEAR, managed adoption deferred | Before converting Bitget sockets to the managed lifecycle, adding a route-rejection seam or handling 40018/40024 on the private socket. | [RUST_BITGET_MANAGED_SOCKET_ROUTING.md](RUST_BITGET_MANAGED_SOCKET_ROUTING.md) |
| Bitget trading write job | Implemented on the streamed serial Plan; stacked on #2092/#2094, size via #2105/#2107 | Before converting the Bitget trading queue, `trade_fail`, `trade_timer` or the `send` body branch to shared Plan authority. | [RUST_BITGET_TRADING_CONTEXT.md](RUST_BITGET_TRADING_CONTEXT.md) |
| Bybit signing | Active dispatch signer adoption | Before Bybit REST/probe/private WS authentication or credential setup changes. | [RUST_BYBIT_DISPATCH_SIGNER.md](RUST_BYBIT_DISPATCH_SIGNER.md) |
| Integer protocol wire stamps | Active shared clock correction | Before formatting outbound timestamps or adopting dispatch signing recipes. | [RUST_PROTOCOL_WIRE_STAMP.md](RUST_PROTOCOL_WIRE_STAMP.md) |
| Shared signer prerequisites | Active engine/provider plan; non-HMAC schema extension in progress | Before family signing migrations, credential decoding, extra signing refs, new provider algorithms, nonce entropy, WS upgrades or signing receipts. | [RUST_SIGNER_PREREQUISITES.md](RUST_SIGNER_PREREQUISITES.md) |
| Non-HMAC signer migrations | Active family adoption plan; shared provider prerequisites first | Before migrating ApeX, Aster, Bitunix, Crypto.com, Orderly, Backpack, Coinbase, Hyperliquid, TxFlow, GRVT, Polymarket, Paradex, Lighter or BreakoutProp signing, or adding provider algorithms. | [RUST_SIGNER_NON_HMAC_MIGRATIONS.md](RUST_SIGNER_NON_HMAC_MIGRATIONS.md) |
| Coinbase signer recipe | Active plan; recipe and provider keypair support, conversion owned by codex-hcmq3f | Before changing Coinbase JWT signing, its dispatch recipe, or the provider's Ed25519 keypair form. | [RUST_COINBASE_SIGNER_RECIPE.md](RUST_COINBASE_SIGNER_RECIPE.md) |
| Remote signing wire v2 and credential import | Plan; Sam-decided P8/P6b, independently reviewed | Before extending sign-req/sign-res, keyless non-HMAC remote signing, browser signOperation, or importing derived credentials (Polymarket L2) into the provider. | [RUST_SIGNER_REMOTE_WIRE_AND_IMPORT.md](RUST_SIGNER_REMOTE_WIRE_AND_IMPORT.md) |
| Read scheduling | Active engine standardization plan | Before changing read coalescing, starvation bounds, shared pauses or startup retry policy. | [RUST_READ_SCHEDULER.md](RUST_READ_SCHEDULER.md) |
| CI Corepack Yarn preparation | Active infrastructure plan | Before changing CI package-manager bootstrap, retries or dependency-cache ordering. | [CI_COREPACK_YARN.md](CI_COREPACK_YARN.md) |
| Cumulative fill derivation | Active engine standardization plan | Before cumulative-to-incremental execution preparation, delivered progress or native fill dedup changes. | [RUST_CUMULATIVE_FILLS.md](RUST_CUMULATIVE_FILLS.md) |
| Financial snapshot freshness | Core + two references merged in #1759; fan-out pending | Before ACK/snapshot publication, native filled/terminal cache, or shared metadata epoch changes. | [RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md](RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md) |
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
| AsterDEX whole-family throughput restoration | In progress | Read before changing its standalone WASM compiler profile or interpreting repeated public timing misses | [RUST_ASTERDEX_THROUGHPUT.md](RUST_ASTERDEX_THROUGHPUT.md) |
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
| TS safe-cex runtime deletion inventory | Prep inventory and a post-processor placement proposal; no deletion | Before deleting the forward-to-worker write path, the worker TS store, algos or post-processors, or moving post-processor outputs | [TS_RUNTIME_DELETION_INVENTORY.md](TS_RUNTIME_DELETION_INVENTORY.md) |
| Downstream contracts | Current caller obligations | SDK/NX caller migration, deployment ordering and deletion prerequisites. | [RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md) (before 2026-10-27) and [callers/](callers/README.md) (one file per change since) |
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
| Standard signing path | Active core implementation | Before dispatch/signature/provider changes or follow-up venue migration. | [RUST_SIGNER.md](RUST_SIGNER.md) |
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
| WEEX ordinary trading Context adoption | Plan and original RED controls | Before migrating ordinary WEEX trading to Preflight and complete Plan admission. | [RUST_WEEX_TRADING_CONTEXT.md](RUST_WEEX_TRADING_CONTEXT.md) |
| WOO perpetual Rust port | Retained plan/evidence | Before changing this venue or the behavior documented in this plan. | [RUST_WOO_PROGRESS.md](RUST_WOO_PROGRESS.md) |
| Shared dispatch signer provider | In progress | Enabling trusted provider construction in another standalone family. | [RUST_SHARED_DISPATCH_PROVIDER.md](RUST_SHARED_DISPATCH_PROVIDER.md) |
| Hosted signer compatibility | In progress: signing-side capture; keyless companion pending | Changing public hosted sign APIs or migrating a family away from raw credentials. | [RUST_HOSTED_SIGNER_BRIDGE.md](RUST_HOSTED_SIGNER_BRIDGE.md) |
| Shared signer setup field policy | In progress | Changing trusted per-family credential transforms or synchronous signing intent. | [RUST_SIGNER_SETUP_POLICY.md](RUST_SIGNER_SETUP_POLICY.md) |
| Bitget public CPU | Historical investigation | Read before changing public parsing or Bitget family throughput profiles | [RUST_BITGET_PUBLIC_CPU.md](RUST_BITGET_PUBLIC_CPU.md) |
| Shared JSON WASM profile | CI experiment | Before changing shared JSON optimization or removing whole-family throughput exceptions. | [RUST_JSON_WASM_PROFILE.md](RUST_JSON_WASM_PROFILE.md) |
| Capability and compiled-feature conformance | Planned CI checker | When changing a capability, command decoder feature or shipped family | [RUST_CAPABILITY_CONFORMANCE.md](RUST_CAPABILITY_CONFORMANCE.md) |
| Managed unsigned HTTP headers | Planned shared prerequisite | Before adapting a native signer with static public headers to managed preparation. | [RUST_UNSIGNED_HTTP_HEADERS.md](RUST_UNSIGNED_HTTP_HEADERS.md) |
| Managed read response authority | Shared prerequisite | Before publishing paired-read responses or managed companion completion events. | [RUST_READ_RESPONSE_AUTHORITY.md](RUST_READ_RESPONSE_AUTHORITY.md) |
| Hyperliquid RequestContext adoption | Inventory and phased plan | Before migrating perps/spot HTTP and completions; public JSON headers, held spot tests and shared signing/plan prerequisites. | [RUST_HYPERLIQUID_REQUEST_CONTEXT.md](RUST_HYPERLIQUID_REQUEST_CONTEXT.md) |
| GRVT RequestContext adoption | Inventory and phased plan | Before migrating GRVT HTTP/completions; public JSON headers, bounded read retries and held account fixture prerequisites. | [RUST_GRVT_REQUEST_CONTEXT.md](RUST_GRVT_REQUEST_CONTEXT.md) |
| Kucoin RequestContext | Committed inventory/plan | Before migrating this venue’s REST or completion authority. | [RUST_KUCOIN_REQUEST_CONTEXT.md](RUST_KUCOIN_REQUEST_CONTEXT.md) |
| Bitrue RequestContext | Committed inventory/plan | Before migrating this venue’s REST or completion authority. | [RUST_BITRUE_REQUEST_CONTEXT.md](RUST_BITRUE_REQUEST_CONTEXT.md) |
| Bitunix RequestContext | Committed inventory/plan | Before migrating this venue’s REST or completion authority. | [RUST_BITUNIX_REQUEST_CONTEXT.md](RUST_BITUNIX_REQUEST_CONTEXT.md) |
| Kraken futures and Spot RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_KRAKEN_REQUEST_CONTEXT.md](RUST_KRAKEN_REQUEST_CONTEXT.md) |
| Coinbase CFM and Spot RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_COINBASE_REQUEST_CONTEXT.md](RUST_COINBASE_REQUEST_CONTEXT.md) |
| Backpack perpetuals RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_BACKPACK_REQUEST_CONTEXT.md](RUST_BACKPACK_REQUEST_CONTEXT.md) |
| Orderly broker family RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_ORDERLY_REQUEST_CONTEXT.md](RUST_ORDERLY_REQUEST_CONTEXT.md) |
| TxFlow testnet perpetuals RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_TXFLOW_REQUEST_CONTEXT.md](RUST_TXFLOW_REQUEST_CONTEXT.md) |
| Polymarket CLOB V2 RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_POLYMARKET_REQUEST_CONTEXT.md](RUST_POLYMARKET_REQUEST_CONTEXT.md) |
| BreakoutProp legacy DXtrade RequestContext | Reviewed inventory and migration plan | Before moving this family from legacy request/completion APIs. | [RUST_BREAKOUTPROP_REQUEST_CONTEXT.md](RUST_BREAKOUTPROP_REQUEST_CONTEXT.md) |
| ApeX Omni RequestContext adoption | Inventory and phased migration plan | Before migrating ApeX HTTP/completion sites or pruning its legacy inventory. | [RUST_APEXOMNI_REQUEST_CONTEXT.md](RUST_APEXOMNI_REQUEST_CONTEXT.md) |
| BloFin managed requests | Inventory and phased adoption plan | Before BloFin HTTP/completion migration; exact legacy map, caller effects and shared API dependencies. | [RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md) |
| BloFin private read funnel | Plan and original controls | Before managing BloFin history, closed-position or account reads, or pruning their ten request fingerprints. | [RUST_BLOFIN_READ_FUNNEL.md](RUST_BLOFIN_READ_FUNNEL.md) |
| BloFin trading write funnel | W1 landed (#2157); W2 barrier join closed by ruling (b), so the serial hedged join is permanent | Before managing BloFin placements, cancels, amends or settings, or pruning its trading request fingerprints. | [RUST_BLOFIN_WRITE_FUNNEL.md](RUST_BLOFIN_WRITE_FUNNEL.md) |

| BloFin credential cutover | Plan in review; WebSocket login onto dispatch, raw options retired | Before changing BloFin's private login, credential options or its credential/signing inventories. | [RUST_BLOFIN_CREDENTIAL_CUTOVER.md](RUST_BLOFIN_CREDENTIAL_CUTOVER.md) |
| Binance signing residue | In review | Before changing Binance's signing tests, fixtures or its signing inventory. | [RUST_BINANCE_SIGNING_RESIDUE.md](RUST_BINANCE_SIGNING_RESIDUE.md) |
| Bitunix hosted signing (P7) | Source in review | Before changing Bitunix remote or hosted signing, its remoteSigning row or the remote signer provider shapes. | [RUST_BITUNIX_HOSTED_SIGNING.md](RUST_BITUNIX_HOSTED_SIGNING.md) |
| Bitunix closed positions | Source in review | Before changing Bitunix journal, closed PnL, its position-history read or the Bitunix capability enums. | [RUST_BITUNIX_CLOSED_POSITIONS.md](RUST_BITUNIX_CLOSED_POSITIONS.md) |
| Binance request refusals | In review | Before changing Binance's local command refusals or its request inventory. | [RUST_BINANCE_REQUEST_REFUSALS.md](RUST_BINANCE_REQUEST_REFUSALS.md) |
| Orderly provider-signed reads | O1 in review; O2 trading writes outlined | Before changing Orderly's signer setup, private account reads, history legs, socket auth or its request/signing inventories. | [RUST_ORDERLY_PROVIDER_READS.md](RUST_ORDERLY_PROVIDER_READS.md) |
| Lighter RequestContext adoption | Inventory and phased plan | Before migrating Lighter or Robinhood HTTP/completion sites; single-request candles first, shared multi-request/signing prerequisites later. | [RUST_LIGHTER_REQUEST_CONTEXT.md](RUST_LIGHTER_REQUEST_CONTEXT.md) |
| Paradex startup Read Contexts | Planned | Independent public catalog/ticker GET adoption and proof | [RUST_PARADEX_SEED_CONTEXT.md](RUST_PARADEX_SEED_CONTEXT.md) |
| BloFin candle command entry | Focused adoption plan | Before removing the candle pre-Context numeric refusal or changing initial command ownership. | [RUST_BLOFIN_CANDLE_ENTRY_CONTEXT.md](RUST_BLOFIN_CANDLE_ENTRY_CONTEXT.md) |
| WEEX dispatch signing | Planned after BingX/shared headers | Before WEEX private managed builders, provider custody or persisted identity cutover. | [RUST_WEEX_DISPATCH_SIGNER.md](RUST_WEEX_DISPATCH_SIGNER.md) |
| TypeScript unit transport isolation | Test isolation | Before adding unit fixtures that could reach fetch, XHR or sockets. | [TS_UNIT_TRANSPORT_ISOLATION.md](TS_UNIT_TRANSPORT_ISOLATION.md) |
| Paradex RequestContext adoption | Inventory and phased plan | Before migrating Paradex HTTP/completions; independent public GET book first, held account/trading and shared JWT/plan prerequisites. | [RUST_PARADEX_REQUEST_CONTEXT.md](RUST_PARADEX_REQUEST_CONTEXT.md) |
| KuCoin Spot managed candles | Focused adoption plan | Before migrating unsigned candle HTTP and command completion to shared authority. | [RUST_KUCOIN_SPOT_CANDLE_CONTEXT.md](RUST_KUCOIN_SPOT_CANDLE_CONTEXT.md) |
| SDK master venue parity | Implementation plan | Before aligning Hyperliquid fee subjects or Lighter Robinhood attribution/client index reservation with master #1869. | [RUST_MASTER_VENUE_PARITY.md](RUST_MASTER_VENUE_PARITY.md) |
| Per-user Rust bundle rollout | Preparation only; Sam-selected bundle switch | Before cohort, client/server engine, frozen-state compatibility or rollback implementation. | [RUST_BUNDLE_ROLLOUT.md](RUST_BUNDLE_ROLLOUT.md) |
| Bundle source readiness matrix | Immutable source inventory; no live certification | Before assessing all-held-venue eligibility or refreshing rollout source evidence. | [rust-bundle-readiness.json](rust-bundle-readiness.json) |

| Shared live Plan admission | Implementation plan | Before consuming Preflight, native whole-plan roots, reserved call slots or owned readiness. | [RUST_LIVE_PLAN_ADMISSION.md](RUST_LIVE_PLAN_ADMISSION.md) |
| KuCoin Spot steady CPU | Profiling plan | Before attributing public steady-state CPU cost or proposing shared-path performance changes. | [RUST_KUCOIN_SPOT_STEADY_PROFILE.md](RUST_KUCOIN_SPOT_STEADY_PROFILE.md) |
| Context-bearing history completion | Original Read-root completion and guarded page admission | Before migrating history admission or finalization to RequestContext ownership. | [RUST_HISTORY_CONTEXT_BRIDGE.md](RUST_HISTORY_CONTEXT_BRIDGE.md) |
| KuCoin futures local history terminals | Focused adoption plan | Before migrating synchronous history/funding preflights and zero-limit completion to shared command authority. | [RUST_KUCOIN_LOCAL_HISTORY_CONTEXT.md](RUST_KUCOIN_LOCAL_HISTORY_CONTEXT.md) |
| Bitunix local history completion | Focused adoption plan | Before migrating synchronous no-I/O history terminals; preserve signed paging and #1771 ownership. | [RUST_BITUNIX_HISTORY_LOCAL_CONTEXT.md](RUST_BITUNIX_HISTORY_LOCAL_CONTEXT.md) |
| Bitrue local history terminals | Focused adoption plan | Before converting synchronous history refusals to genuine shared Read command authority. | [RUST_BITRUE_LOCAL_HISTORY_CONTEXT.md](RUST_BITRUE_LOCAL_HISTORY_CONTEXT.md) |
| Bitrue local leverage preflights | Focused adoption plan | Before converting synchronous unknown-market/absent-credential leverage refusals to shared command authority. | [RUST_BITRUE_LOCAL_LEVERAGE_CONTEXT.md](RUST_BITRUE_LOCAL_LEVERAGE_CONTEXT.md) |
| Phemex local history command Contexts | Plan; original REDs next | Before changing synchronous history/income terminals or their engine admission. | [RUST_PHEMEX_LOCAL_HISTORY_CONTEXT.md](RUST_PHEMEX_LOCAL_HISTORY_CONTEXT.md) |
| MEXC synchronous read command custody | Draft adoption; local checks pass | Before changing local history/leverage refusals, genuine command entry or the exact eight-site prune | [RUST_MEXC_LOCAL_READ_CONTEXT.md](RUST_MEXC_LOCAL_READ_CONTEXT.md) |

| Paradex public-trades Read history | Consumer plan | Before migrating the native unsigned single-page public trade command through the reviewed history bridge. | [RUST_PARADEX_PUBLIC_TRADES_CONTEXT.md](RUST_PARADEX_PUBLIC_TRADES_CONTEXT.md) |
| KuCoin local Read terminals | Frozen-base adoption plan | Before converting Spot cached/empty/refusal reads and futures unsupported leverage to genuine initial Read authority. | [RUST_KUCOIN_LOCAL_READ_CONTEXT.md](RUST_KUCOIN_LOCAL_READ_CONTEXT.md) |
| BloFin PublicTrades Context | Committed native consumer plan | Before moving the single unsigned trade-history response onto the shared Read completion bridge. | [RUST_BLOFIN_PUBLIC_TRADES_CONTEXT.md](RUST_BLOFIN_PUBLIC_TRADES_CONTEXT.md) |
| Bitunix public REST Read authority | Managed adoption; native proof | Before moving public markets/tickers/candles, extracting native codes or pruning their eight legacy occurrences. | [RUST_BITUNIX_PUBLIC_READ_CONTEXT.md](RUST_BITUNIX_PUBLIC_READ_CONTEXT.md) |
| WEEX local history custody | Committed plan; unimplemented | Before adopting synchronous WEEX history results/refusals or pruning their seven legacy terminals. | [RUST_WEEX_LOCAL_HISTORY_CONTEXT.md](RUST_WEEX_LOCAL_HISTORY_CONTEXT.md) |
| WEEX account identity Read custody | Plan; original REDs next | Before changing FetchAccount local refusals or initial managed identity validation. | [RUST_WEEX_IDENTITY_CONTEXT.md](RUST_WEEX_IDENTITY_CONTEXT.md) |
| Binance identity preflight terminals | Focused adoption plan | Before converting local identity refusals to genuine Read custody; preserve selected signing and session ownership. | [RUST_BINANCE_IDENTITY_TERMINALS.md](RUST_BINANCE_IDENTITY_TERMINALS.md) |
| Selection-managed signed reads | Standalone engine implementation; consumers separate | Before coupling selected identity/egress/evidence with managed signed Read admission, queueing or retry. | [RUST_SELECTED_MANAGED_READ.md](RUST_SELECTED_MANAGED_READ.md) |

| Raw signature expiry at final release | Plan-first engine prerequisite | Before publishing engine-projected raw signature expiry to a repair/reconciliation consumer. | [RUST_SIGNING_RELEASE_EXPIRY.md](RUST_SIGNING_RELEASE_EXPIRY.md) |
| OKX dispatch signer migration | Planned | OKX opaque identities, send-time HTTP/WS and hosted caller adoption | [RUST_OKX_DISPATCH_SIGNER.md](RUST_OKX_DISPATCH_SIGNER.md) |
| OKX caller adoption (leverage, account GET, trade entry) | Plan; first no-bridge slice | Before converting OKX FetchLeverage, account config GET or trade_command entry refusals to shared Context authority. | [RUST_OKX_CALLER_ADOPTION.md](RUST_OKX_CALLER_ADOPTION.md) |
| OKX caller adoption, slice 2 (candles) | Plan + original REDs; no production change | Before converting OKX FetchCandles (recent and history-candles legs) to shared command Context authority. | [RUST_OKX_CALLER_CANDLES.md](RUST_OKX_CALLER_CANDLES.md) |
| Read Plan receipt cap | Receipt-cap implementation under review on frozen #1945 | Before bounding Plan admission by the original command receipt across JWT/preflight waits. | [RUST_PLAN_RECEIPT_CAP.md](RUST_PLAN_RECEIPT_CAP.md) |
| Preflight inherited uncertainty | Plan under review (uufub4, dazzy3) | Before a WriteAfterReads retry of a maybe-sent intent seals without a released attempt. | [RUST_PREFLIGHT_INHERITED_UNCERTAINTY.md](RUST_PREFLIGHT_INHERITED_UNCERTAINTY.md) |
| Plan session wait | Slot, retry and Preflight (N7) park implemented; N7 review pending (zs5e83, m26ps4) | Before a command Plan or retained Preflight leg must wait for a venue login Session. | [RUST_PLAN_SESSION_WAIT.md](RUST_PLAN_SESSION_WAIT.md) |
| Streamed serial Plan | REDs frozen; source next | Before an ordered write funnel (OKX, Bitget) streams calls with no declared count. | [RUST_PLAN_SERIAL_STREAM.md](RUST_PLAN_SERIAL_STREAM.md) |
| Condition wait | Plan; ruling and review before REDs | Before a venue parks a Preflight or serial Plan on a venue condition with its own bound (Bitget detection). | [RUST_CONDITION_WAIT.md](RUST_CONDITION_WAIT.md) |
| Command decoder size | Plan and wire-compat harness; review before source | Before replacing the internally tagged CommandIn decoder with a borrowed tag scan. | [RUST_COMMAND_DECODER.md](RUST_COMMAND_DECODER.md) |
| BingX FetchAccount selected Read | Plan; original REDs next | Before replacing identity entry/failure custody with the frozen shared selected-read helper or pruning its two terminals. | [RUST_BINGX_IDENTITY_CONTEXT.md](RUST_BINGX_IDENTITY_CONTEXT.md) |
| Local fixed-setting Mutation success | Plan; API review before original REDs | Before replacing keyless KuCoin Spot and Bitrue fixed-setting successes with genuine-entry local completion. | [RUST_LOCAL_SETTING_SUCCESS.md](RUST_LOCAL_SETTING_SUCCESS.md) |
| [Bitget CPU option-B follow-up](RUST_BITGET_CPU_OPTION_B_FOLLOWUP.md) | Shared second-runner CPU remeasurement, two-run mean and fail-closed certification; enabled for every CPU-gated suite, with unchanged tapes and thresholds. |
| Caller-wait Read root expiry | Plan under review (claude-fe7lgm) | Before relying on engine-owned expiry of a tracked caller-wait Read root, or refusing a missing command receipt. | [RUST_READ_ROOT_EXPIRY.md](RUST_READ_ROOT_EXPIRY.md) |
| Command-entry WASM size | Measured; step 1 in PR | Before adding engine code on a family's first command-entry admission, or chasing per-site entry cost. | [RUST_COMMAND_ENTRY_SIZE.md](RUST_COMMAND_ENTRY_SIZE.md) |
| Engine core size | Session-leg split and ID-map hasher under review | Before a family's first RequestContext adoption pushes it over budget. | [RUST_ENGINE_CORE_SIZE.md](RUST_ENGINE_CORE_SIZE.md) |
| Store order fence domains | Keep-both documentation plan | Before changing the broad late-ACK revision or selective logical snapshot epoch. | [RUST_ORDER_FENCE_DOMAINS.md](RUST_ORDER_FENCE_DOMAINS.md) |
| Selected Read roots and page legs | Shared implementation; final dependency integration pending | Before retaining one selected caller-wait root across native physical history pages. | [RUST_SELECTED_READ_LEGS.md](RUST_SELECTED_READ_LEGS.md) |

| TxFlow whole managed socket family | Managed adapter | Before converting api-ws and node ws together using shared pacing and subscription handoff; no new socket seam. | [RUST_TXFLOW_MANAGED_SOCKETS.md](RUST_TXFLOW_MANAGED_SOCKETS.md) |
| TxFlow steady ticker CPU | Profiling and code-only optimization | Before changing TxFlow ticker row lookup, borrowed symbols or its CPU evidence. | [RUST_TXFLOW_STEADY_CPU.md](RUST_TXFLOW_STEADY_CPU.md) |

| Replay protocol audit finalization | Shared harness correction | Before changing checkpoint capture, terminal protocol coverage or the final replay boundary. | [RUST_REPLAY_AUDIT_FINALIZATION.md](RUST_REPLAY_AUDIT_FINALIZATION.md) |
| OKX history request custody | Plan and original REDs; waits for #1945 | Before converting OKX history transport, native soft failure or caller-root deadlines. | [RUST_OKX_HISTORY_REQUEST_CONTEXT.md](RUST_OKX_HISTORY_REQUEST_CONTEXT.md) |
| Crypto.com read command Contexts | Planned | Before public candle or local history adoption; exact nine-occurrence/seven-entry shrink and original RED controls | [RUST_CRYPTOCOM_READ_COMMAND_CONTEXT.md](RUST_CRYPTOCOM_READ_COMMAND_CONTEXT.md) |
| Crypto.com public candle and local history Contexts | Implementation under review | Before converting unsigned candles and synchronous history terminals or pruning their seven inventory keys. | [RUST_CRYPTOCOM_PUBLIC_READ_CONTEXT.md](RUST_CRYPTOCOM_PUBLIC_READ_CONTEXT.md) |

| Lighter synchronous history Read terminals | Plan; original REDs next | Before changing local history validation/empty completion custody or pruning its seven sites. | [RUST_LIGHTER_LOCAL_HISTORY_CONTEXT.md](RUST_LIGHTER_LOCAL_HISTORY_CONTEXT.md) |

| GRVT synchronous history Read terminals | Plan; original REDs next | Before changing local history refusal custody or pruning its seven sites. | [RUST_GRVT_LOCAL_HISTORY_CONTEXT.md](RUST_GRVT_LOCAL_HISTORY_CONTEXT.md) |
| KuCoin local command refusals | Implemented; focused checks/reviews clear, CI pending | Before migrating the eight synchronous pre-I/O refusals while preserving valid signed paths and successful local setting policy. | [RUST_KUCOIN_LOCAL_COMMAND_REFUSALS.md](RUST_KUCOIN_LOCAL_COMMAND_REFUSALS.md) |
| Bitrue local command refusals | Implemented; focused checks/reviews clear, CI pending | Before migrating three initial synchronous trading refusals while preserving the queued/private path and successful one-way setting. | [RUST_BITRUE_LOCAL_COMMAND_REFUSALS.md](RUST_BITRUE_LOCAL_COMMAND_REFUSALS.md) |
| GRVT replay diagnostics | Plan and local controls | Before narrowing exact tape-bound protocol declarations; preserve genuine final errors and all other diagnostics. | [RUST_GRVT_REPLAY_DIAGNOSTICS.md](RUST_GRVT_REPLAY_DIAGNOSTICS.md) |
| Crypto.com replay diagnostics | Plan and local controls | Before narrowing exact private-stream declarations; preserve receiver-specific channels and genuine final errors. | [RUST_CRYPTOCOM_REPLAY_DIAGNOSTICS.md](RUST_CRYPTOCOM_REPLAY_DIAGNOSTICS.md) |

- [Binance Spot replay protocol diagnostics](RUST_BINANCESPOT_REPLAY_DIAGNOSTICS.md) — source-bound audit controls and exact declaration pruning, preserving native failures.
| MEXC local write refusal Contexts | Implementation under review | Before converting pre-I/O validation/missing-key refusals or deleting the duplicate unreachable Err terminal. | [RUST_MEXC_LOCAL_WRITE_CONTEXT.md](RUST_MEXC_LOCAL_WRITE_CONTEXT.md) |

| KuCoin Spot fixed-setting completion | Shared LocalSetting adoption; independent reviews complete | Before changing native keyless fixed-setting successes and their entry refusal custody. | [RUST_KUCOIN_SPOT_LOCAL_SETTINGS.md](RUST_KUCOIN_SPOT_LOCAL_SETTINGS.md) |

| Bitrue local one-way setting | Shared LocalSetting adoption under review | Before replacing the pre-credential SetPositionMode(false) success with shared LocalSetting authority. | [RUST_BITRUE_LOCAL_POSITION_MODE.md](RUST_BITRUE_LOCAL_POSITION_MODE.md) |

| [WEEX account-cycle Background reads](RUST_WEEX_ACCOUNT_BACKGROUND.md) | Managed Background account pages; draft validation |
| Flat-position realized PnL evidence | Shared harness correction | Before projecting unavailable native PnL or pruning inverse flat-position diagnostics. | [RUST_FLAT_POSITION_PNL_EVIDENCE.md](RUST_FLAT_POSITION_PNL_EVIDENCE.md) |

| Paradex public refusals and Lighter catalog | Implementation under review; original REDs recorded | Before converting the last four immediately workable public request sites. | [RUST_PUBLIC_READ_TAIL.md](RUST_PUBLIC_READ_TAIL.md) |

| BloFin local leverage Read refusals | Shared Read admission adoption under review | Before changing the two pre-credential local FetchLeverage refusal sites independently of signer cutover. | [RUST_BLOFIN_LOCAL_LEVERAGE_REFUSALS.md](RUST_BLOFIN_LOCAL_LEVERAGE_REFUSALS.md) |

| Polymarket/Kraken fixed settings | LocalSetting adoption | Before changing the two held keyless fixed-setting success sites. | [RUST_FIXED_SETTING_ADOPTION.md](RUST_FIXED_SETTING_ADOPTION.md) |
| OKX reported closed-position Read custody | Implemented; native/strict/reviews clear, CI pending | Before migrating request_closed_page from legacy Completion/list to the caller-root/history bridge. | [RUST_OKX_CLOSED_HISTORY_CONTEXT.md](RUST_OKX_CLOSED_HISTORY_CONTEXT.md) |


| Native socket snapshot barrier | Shared prerequisite API plan | Before separating login acceptance from a connect-origin native snapshot completion; no venue timer or second reducer. | [RUST_SOCKET_SNAPSHOT_BARRIER.md](RUST_SOCKET_SNAPSHOT_BARRIER.md) |
| Whole Phemex managed sockets | Whole-family adoption plan | Before changing public/private transport, auth or accepted snapshot expiry; preserves the parked public controls and REST-owner boundaries. | [RUST_PHEMEX_MANAGED_SOCKETS.md](RUST_PHEMEX_MANAGED_SOCKETS.md) |



| Phemex dispatch HMAC custody | Full cutover reviewed; folds into #1974 after #1970 | Before changing Phemex REST/WS canonical signing, provider setup or reconciliation expiry | [RUST_PHEMEX_DISPATCH_SIGNING.md](RUST_PHEMEX_DISPATCH_SIGNING.md) |
| MEXC unsigned discovery and depth ownership | Managed adoption under review | Before adopting the two public Background Read send sites. | [RUST_MEXC_PUBLIC_BACKGROUND_CONTEXT.md](RUST_MEXC_PUBLIC_BACKGROUND_CONTEXT.md) |

| Hyperliquid spot candle custody | Managed adoption; focused controls pass, review and CI pending | Before changing standalone or merged spot candles; reuses the existing perps Context and wire path. | [RUST_HYPERLIQUID_SPOT_CANDLES_CONTEXT.md](RUST_HYPERLIQUID_SPOT_CANDLES_CONTEXT.md) |
| Aster frozen startup clock boundary | Test-only plan and deterministic control | Before assuming startup readiness occurs at a fixed post-sync timestamp. | [RUST_ASTERDEX_REPLAY_CLOCK_BOUNDARY.md](RUST_ASTERDEX_REPLAY_CLOCK_BOUNDARY.md) |
| Crypto.com immediate trading custody | Implemented; native/strict/reviews pass, CI pending | Before converting the eight local refusal/fixed-setting sites. | [RUST_CRYPTOCOM_LOCAL_TRADING_CONTEXT.md](RUST_CRYPTOCOM_LOCAL_TRADING_CONTEXT.md) |
| Crypto.com public discovery ownership | Managed implementation under review | Before managing the last unsigned discovery sender and its Markets/Tickers continuations. | [RUST_CRYPTOCOM_PUBLIC_DISCOVERY_CONTEXT.md](RUST_CRYPTOCOM_PUBLIC_DISCOVERY_CONTEXT.md) |

| Orderly initial history refusals | Implemented; bounded reviews and CI pending | Before converting the three initial history refusals; excludes combo-5b signer/source work. | [RUST_ORDERLY_LOCAL_COMMAND_CONTEXT.md](RUST_ORDERLY_LOCAL_COMMAND_CONTEXT.md) |

| Kraken Futures public candle custody | Managed adoption; focused controls pass, source review/CI pending | Before changing caller-root pages or subscription candle polling. | [RUST_KRAKEN_FUTURES_CANDLE_CONTEXT.md](RUST_KRAKEN_FUTURES_CANDLE_CONTEXT.md) |
| Crypto.com unsigned PublicTrades custody | Plan and original controls | Before replacing the last unsigned PublicTrades history sender independently of private signing. | [RUST_CRYPTOCOM_PUBLIC_TRADES_CONTEXT.md](RUST_CRYPTOCOM_PUBLIC_TRADES_CONTEXT.md) |

| Breakout immediate local command custody | Plan and original-source controls | Before adopting seven initial local terminals or changing pre-venue/selection boundaries. | [RUST_BREAKOUT_LOCAL_COMMAND_CONTEXT.md](RUST_BREAKOUT_LOCAL_COMMAND_CONTEXT.md) |

| Orderly unsupported order controls | Implemented; source reviews and CI pending | Before converting the one initial Bybit BBO/TickSize refusal; no deferred-job or signer changes. | [RUST_ORDERLY_UNSUPPORTED_ORDER_CONTEXT.md](RUST_ORDERLY_UNSUPPORTED_ORDER_CONTEXT.md) |

- [Paradex config/local entry](RUST_PARADEX_LOCAL_ENTRY.md) and [remaining engine seams](RUST_PARADEX_ENGINE_GAPS.md): seven-site managed/local slice after the hosted signing decision.

| Lighter unsigned identity Read | Managed implementation under review | Before changing FetchAccount candidate custody, original caller expiry or the exact three-site identity prune. | [RUST_LIGHTER_IDENTITY_CONTEXT.md](RUST_LIGHTER_IDENTITY_CONTEXT.md) |

| Hyperliquid local settings and schedule refusals | Shared local terminal adoption under review | Before changing the nine synchronous pre-signing local terminals or their exact inventory prune. | [RUST_HYPERLIQUID_LOCAL_SETTINGS_CONTEXT.md](RUST_HYPERLIQUID_LOCAL_SETTINGS_CONTEXT.md) |
| Multi-constraint release pacing | Shared engine prerequisite; plan and RED phase | Before composing several HTTP rate groups at one Claim or signing after pacing waits. | [RUST_MULTI_CONSTRAINT_PACING.md](RUST_MULTI_CONSTRAINT_PACING.md) |

| Kraken Futures unsigned PublicTrades ownership | Managed implementation; focused proof and source review in progress | Before changing caller-root cursor pages or accepted-only failure recovery. | [RUST_KRAKEN_FUTURES_PUBLIC_TRADES_CONTEXT.md](RUST_KRAKEN_FUTURES_PUBLIC_TRADES_CONTEXT.md) |

| Receipt-bound stamp floor | Focused gates/reviews clear; full CI pending | Before monotonic per-request signature or transport reissues; floor grants no resend authority. | [RUST_STAMP_FLOOR.md](RUST_STAMP_FLOOR.md) |

| Bitrue dispatch signing | Production candidate; original REDs green, review and CI pending | Before replacing inline Bitrue HMAC, selection or native signature resend. | [RUST_BITRUE_DISPATCH_SIGNER.md](RUST_BITRUE_DISPATCH_SIGNER.md) |
| Bitrue request funnel and trading writes | Plan under review | Before converting Bitrue's HTTP funnel, account reads, listen key, lookup/leverage joins or trading writes. | [RUST_BITRUE_REQUEST_FUNNEL.md](RUST_BITRUE_REQUEST_FUNNEL.md) |
| Bitunix dispatch signer | Source under review; plan CLEAR (dazzy3) | Before moving Bitunix REST or WS login signing off raw key/secret, or changing its identities or registry setup. | [RUST_BITUNIX_DISPATCH_SIGNER.md](RUST_BITUNIX_DISPATCH_SIGNER.md) |

| GRVT unsigned candle Read Plan | Managed adoption under review | Before changing candle caller limits, streaming or the six-attempt transport budget. | [RUST_GRVT_CANDLE_CONTEXT.md](RUST_GRVT_CANDLE_CONTEXT.md) |

| GRVT unsigned book command | Managed source under review; stacked after candle draft | Before changing independent FetchOrderBook command ownership, native errors or the six-attempt transport budget. | [RUST_GRVT_BOOK_QUERY_CONTEXT.md](RUST_GRVT_BOOK_QUERY_CONTEXT.md) |

| TxFlow immediate local command custody | Draft adoption; original controls green | Before migrating local history/candle/trading completion or composing with the ticker CPU slice. | [RUST_TXFLOW_LOCAL_COMMAND_CONTEXT.md](RUST_TXFLOW_LOCAL_COMMAND_CONTEXT.md) |
| OKX history selected identity | Implemented; scoped checks pass, review pending | Before removing the closed/history credential-selection legacy sites. | [RUST_OKX_HISTORY_SELECTED_IDENTITY.md](RUST_OKX_HISTORY_SELECTED_IDENTITY.md) |
| Order-lock capacity refusal | Implemented; scoped checks pass, source review pending | Before changing order-lock entry refusals or the 64-scope cap. | [RUST_ORDER_LOCK_CAPACITY.md](RUST_ORDER_LOCK_CAPACITY.md) |

- [Polymarket midpoint seed custody](RUST_POLYMARKET_MIDPOINT_READ_CONTEXT.md): one unsigned Background Read site; plan and original controls before production.

| GRVT fixed local settings | Shared local terminal adoption under review | Before changing the three synchronous pre-credential settings terminals or their exact inventory prune. | [RUST_GRVT_LOCAL_SETTINGS_CONTEXT.md](RUST_GRVT_LOCAL_SETTINGS_CONTEXT.md) |

| GRVT initial leverage Read refusals | Shared local terminal adoption under review | Before changing the two pre-issuance leverage refusals or their exact account inventory prune. | [RUST_GRVT_LOCAL_LEVERAGE_REFUSALS.md](RUST_GRVT_LOCAL_LEVERAGE_REFUSALS.md) |


| GRVT unsigned instrument catalog custody | Managed implementation; source review and CI pending | Before changing startup/refresh publication, the new 20-second bound or the dead options-arm deletion. | [RUST_GRVT_CATALOG_CONTEXT.md](RUST_GRVT_CATALOG_CONTEXT.md) |

| TxFlow command candle snapshot | Managed source under review; 19 dedicated controls pass | Before managing the unsigned command candle read; excludes initial local refusals, discovery and account. | [RUST_TXFLOW_CANDLE_CONTEXT.md](RUST_TXFLOW_CANDLE_CONTEXT.md) |
| BloFin managed HMAC dispatch recipe | Plan and original native controls | Before preparing the signing-entropy nonce and opaque Secondary-or-Primary GET recipe for FetchLeverage. | [RUST_BLOFIN_HMAC_DISPATCH_RECIPE.md](RUST_BLOFIN_HMAC_DISPATCH_RECIPE.md) |

| Command-owned Read-Plan history authority | Implemented; source reviews and focused checks clear, CI pending | Before accepting Plan pages, settling Slots with accepted rows or recovering a released transport failure through the same Plan terminal. | [RUST_READ_PLAN_HISTORY_AUTHORITY.md](RUST_READ_PLAN_HISTORY_AUTHORITY.md) |

| Independent Mutation Plan calls | Plan-first engine follow-up after #1932 | Before continuing independent native batch siblings after a delivered unproven 408/5xx. | [RUST_PLAN_INDEPENDENT_FAILURES.md](RUST_PLAN_INDEPENDENT_FAILURES.md) |
| Parallel Mutation preparation | Engine/host and pacing reviewed; CI pending | Before releasing any parallel mutation leg while sibling preparation/signing is outstanding. | [RUST_PLAN_MUTATION_PREPARE_BARRIER.md](RUST_PLAN_MUTATION_PREPARE_BARRIER.md) |
| AsterDEX local Read custody and lane remainder | Implementation under review | Before converting Aster synchronous history/leverage/candle refusals or selecting the next lane legacy-site slice. | [RUST_ASTERDEX_LOCAL_READ_CONTEXT.md](RUST_ASTERDEX_LOCAL_READ_CONTEXT.md) |
| AsterDEX local trading refusal custody | Implemented; native/strict/reviews clear, CI pending | Before converting the three shared Read/Mutation refusals or moving acknowledged cleanup behind admission. | [RUST_ASTERDEX_LOCAL_TRADE_REFUSALS.md](RUST_ASTERDEX_LOCAL_TRADE_REFUSALS.md) |
| Lighter local trading refusal custody | Plan and REDs; reviewer dazzy3 | Before converting Lighter's synchronous trade_command refusals or SetPositionMode's local success. | [RUST_LIGHTER_LOCAL_TRADE_REFUSALS.md](RUST_LIGHTER_LOCAL_TRADE_REFUSALS.md) |
| Phemex identity and recurring account read owners | Implemented on actual combo-5b; source review and CI pending | Before SelectedRead identity, Background currency cycles or Primary order reconciliation. | [RUST_PHEMEX_READ_OWNERS.md](RUST_PHEMEX_READ_OWNERS.md) |

| Managed socket authentication progress | Plan before original REDs; runtime pending review | Before Kraken challenge progress renews auth without granting readiness; whole-family consumer remains held. | [RUST_SOCKET_AUTH_PROGRESS.md](RUST_SOCKET_AUTH_PROGRESS.md) |

| TxFlow unsigned ticker seed custody | Local plan and original-control checkpoint | Before managing the activeAssetCtx seed sender; preserve native seed cadence, keyed retry and CPU cache. | [RUST_TXFLOW_TICKER_SEED_CONTEXT.md](RUST_TXFLOW_TICKER_SEED_CONTEXT.md) |


| GRVT remaining local command terminals | Managed local refusal and dead dispatch deletion; review/CI pending | Before changing generic Unsupported custody, Mutation outcome metadata or dead private-phase coverage. | [RUST_GRVT_LOCAL_COMMAND_TAIL.md](RUST_GRVT_LOCAL_COMMAND_TAIL.md) |

| PublicExchangeFactory native startup fixture | Fixture corrected; scoped native/unit checks pass, review pending | Before changing the native server-adapter timeout fixture or its preservation assertions. | [RUST_PUBLIC_FACTORY_STARTUP_TEST.md](RUST_PUBLIC_FACTORY_STARTUP_TEST.md) |

| CPU evidence transport retrieval retry | Plan and originals | Before changing GitHub evidence retrieval retries; provenance and CPU verdicts stay exact. | [RUST_CPU_EVIDENCE_TRANSPORT_RETRY.md](RUST_CPU_EVIDENCE_TRANSPORT_RETRY.md) |
| Coinbase local command custody | Draft adoption; ten sites and original controls pass | Before adopting immediate local completions or distinguishing deferred/queued public-read gates. | [RUST_COINBASE_LOCAL_CONTEXT.md](RUST_COINBASE_LOCAL_CONTEXT.md) |
| Orderly public seed custody | Plan and original REDs; source held on actual 5b | Before managing the shared unsigned markets/tickers seed sender and its per-leg timeout. | [RUST_ORDERLY_PUBLIC_SEED_CONTEXT.md](RUST_ORDERLY_PUBLIC_SEED_CONTEXT.md) |
| Crypto.com full private dispatch and RequestContext | Plan and original controls; shared source after 5b | Before removing the final eight private witnesses and raw signing funnel. | [RUST_CRYPTOCOM_PRIVATE_REQUEST_CONTEXT.md](RUST_CRYPTOCOM_PRIVATE_REQUEST_CONTEXT.md) |
| Metadata/settings freshness | Opt-in isolation implemented; source/OFF-row reviews and local size gates clear; fresh CI pending | Before metadata read coalescing, per-symbol write fences, atomic publication or family feature/size isolation. | [RUST_METADATA_FRESHNESS.md](RUST_METADATA_FRESHNESS.md) |

| TxFlow unsigned account-setting poll custody | Managed source reviewed; scoped controls pass, fresh CI pending | Before managing activeAssetData; preserve private routing, native cadence and financial revision fences. | [RUST_TXFLOW_ACCOUNT_SETTING_CONTEXT.md](RUST_TXFLOW_ACCOUNT_SETTING_CONTEXT.md) |

| Polymarket Gamma catalogue custody | Managed adoption; original and pressure controls pass, review/CI pending | Before adopting unsigned streamed catalogue pages; retain atomic cursor walk and native retry. | [RUST_POLYMARKET_CATALOGUE_CONTEXT.md](RUST_POLYMARKET_CATALOGUE_CONTEXT.md) |

| Backpack supported-history Read custody | Source reviews CLEAR; full CI next | Before moving four history routes onto one caller-bound Read owner and activating publicRead. | [RUST_BACKPACK_HISTORY_READ_CUSTODY.md](RUST_BACKPACK_HISTORY_READ_CUSTODY.md) |

| Paradex subscription candle poll custody | Source and local size gates CLEAR; qualifying CI pending | Before changing poll Background Read ownership, stale generations, native fallback or completion/recovery timers. | [RUST_PARADEX_CANDLE_POLL_CONTEXT.md](RUST_PARADEX_CANDLE_POLL_CONTEXT.md) |

| Backpack public-seed Read custody | Source/scoped checks CLEAR; exact-head CI pending | Before managing Markets, tickers and MarkPrices startup/recovery observations. | [RUST_BACKPACK_PUBLIC_SEED_READ_CUSTODY.md](RUST_BACKPACK_PUBLIC_SEED_READ_CUSTODY.md) |

| [Orderly local candle and identity completions](RUST_ORDERLY_LOCAL_READ_COMPLETIONS.md) | Managed source; 19 originals and 3 expiry controls pass, 23F/24O to 17F/18O; source review and CI pending. |

| Dispatch nonce reservation and release ordering | Source/review/local gates CLEAR; refreshed exact-head CI pending | Before reserving a selected-key nonce in SignStep preparation or changing nonce receipt and release-floor custody. | [RUST_DISPATCH_NONCE_RESERVATION.md](RUST_DISPATCH_NONCE_RESERVATION.md) |

| Backpack account GET Read custody | Plan CLEAR; originals reproduced, review pending | Before managing Info, Positions, Balance and Orders observations; preserve native snapshots and continuation order. | [RUST_BACKPACK_ACCOUNT_READ_CUSTODY.md](RUST_BACKPACK_ACCOUNT_READ_CUSTODY.md) |
| Kraken Futures and Spot dispatch recipes | Draft recipes on nonce prerequisite; native caller adoption held | Before adding HMAC-SHA512 programs, trusted Base64 setup or nonce-bound selection. | [RUST_KRAKEN_DISPATCH_SIGNER.md](RUST_KRAKEN_DISPATCH_SIGNER.md) |

| Serial single-call schedule construction | Local cut; paired measurements and exact-patch review pending | Before changing serial topology construction or declared-admission error precedence. | [RUST_SERIAL_SINGLE_CALL_SCHEDULE.md](RUST_SERIAL_SINGLE_CALL_SCHEDULE.md) |

| Journal report iterator simplification | [RUST_JOURNAL_REPORT_ITERATORS.md](RUST_JOURNAL_REPORT_ITERATORS.md) | codex-ih35js | Local measured cut after Bitget slice 3; exact review required. |

| Phemex remaining request funnel | [RUST_PHEMEX_REMAINING_FUNNEL.md](RUST_PHEMEX_REMAINING_FUNNEL.md) | codex-ih35js | Source correction: native gate precedence and income prefix recovery; source review/CI and composed landing size gates pending; four named repair holdouts. |

| Shared hosted-signing envelope | Docs-only companion; required m26ps4 review; Paradex batch row amended by the overseer's ruling | Before shared Kraken HMAC or Paradex Stark structured-preimage transport, provider/capture or capability changes. | [RUST_HOSTED_SIGNING_ENVELOPE.md](RUST_HOSTED_SIGNING_ENVELOPE.md) |

| Paradex hosted Stark consumer (mode 2) | Plan in review (codex-9czdjh, claude-kuhwxi); #2126 HELD | Before the Paradex descriptor, digest export, delegated-path cutover or device consumer. | [RUST_PARADEX_HOSTED_STARK_CONSUMER.md](RUST_PARADEX_HOSTED_STARK_CONSUMER.md) |

| OKX ordered write ownership | Source GO (dazzy3); exact-head CI green | Before retiring the final ten OKX request/completion sites. | [RUST_OKX_ORDERED_WRITE_CONTEXT.md](RUST_OKX_ORDERED_WRITE_CONTEXT.md) |

| Serial Session resend | Originals CLEAR; source review and paired size gates pending | Before a serial call's one authentication resend waits for a login round trip (GRVT code 1000). | [RUST_SERIAL_SESSION_RESEND.md](RUST_SERIAL_SESSION_RESEND.md) |
| Backpack public and private socket lifecycle | Source/API/native CLEAR; D1 local gates and combined size green; family record/CI pending | Before adopting Backpack feed epochs/interests, native private auth lifecycle, or enabling managed-sockets. | [RUST_BACKPACK_SOCKET_LIFECYCLE.md](RUST_BACKPACK_SOCKET_LIFECYCLE.md) |

| Hyperliquid trading ownership and EIP-712 dispatch | Plan first; 15 frozen entries, actual-agent nonce/setup/pacing gates OPEN | Before adopting the shared trading FIFO, signed /exchange, amendment or journal terminals. | [RUST_HYPERLIQUID_TRADING_CONTEXT.md](RUST_HYPERLIQUID_TRADING_CONTEXT.md) |

| GRVT serial write funnel | Plan/originals CLEAR; shared serial resend and Session support landed; native cutover held on G3/G6 | Before changing the five trading terminals, FIFO custody, signing or cached-session headers. | [RUST_GRVT_WRITE_FUNNEL.md](RUST_GRVT_WRITE_FUNNEL.md) |

| GRVT EIP712 provider recipe and installed construction | Local recipe source CLEAR; D1 carry review; installed construction held on G3/G6 | Before installing the minimal hosted/local signing inputs, removing raw GRVT keys, or composing serial resend with Session headers. | [RUST_GRVT_PROVIDER_RECIPE.md](RUST_GRVT_PROVIDER_RECIPE.md) |
| Engine signing metadata | Source CLEAR (m26ps4, 9czdjh); sizes and CI pending | Before exporting signing-task metadata or carrying a CredentialRole into managed admission. | [RUST_ENGINE_SIGN_META.md](RUST_ENGINE_SIGN_META.md) |

| WOO managed socket adoption | Native codec over shared lifecycle | Before modifying WOO epoch, auth, interest replay, retirement or supervision. | [RUST_WOO_MANAGED_SOCKETS.md](RUST_WOO_MANAGED_SOCKETS.md) |
| WOO socket adoption and request funnel | Source/API/native/map carry CLEAR; first-source paired sizes fit, current combined and CI pending | Before importing reviewed WOO sockets or converting its 30 request / 20 signing sites. | [RUST_WOO_ADOPTION_AND_FUNNEL.md](RUST_WOO_ADOPTION_AND_FUNNEL.md) |

- [KuCoin request and signing funnel](RUST_KUCOIN_FUNNEL.md): predecessor preservation, native refusal resend and compiling custody originals; production adoption remains local.
| Retire an unissued independent Plan call | Plan/originals CLEAR (dazzy3, 4yjlw0); source review pending | Before a venue skips a declared complete-Plan call it decides not to send (Bitrue B2 cancel batches). | [RUST_PLAN_RETIRE_UNISSUED.md](RUST_PLAN_RETIRE_UNISSUED.md) |
| Per-call pacing on serial Plans | Plan/originals/source in one engine PR; review pending | Before a serial Plan mixes request kinds with different pacing (WOO trade FIFO, MEXC submit jobs). | [RUST_SERIAL_CALL_PACING.md](RUST_SERIAL_CALL_PACING.md) |

| WOO unsigned public-seed read custody | Local source/fixtures CLEAR;264PASS, sizes fit; unpushed, CI pending | Before converting Markets, Tickers or Funding to independent Background Read observations. | [RUST_WOO_PUBLIC_SEED_READ_CUSTODY.md](RUST_WOO_PUBLIC_SEED_READ_CUSTODY.md) |

| Bitget history Read funnel | Source CLEAR; correctness green; direct overseer landing GO | Before changing history caller bounds, paging, promotion, retry custody or selected identity. | [RUST_BITGET_HISTORY_READ_FUNNEL.md](RUST_BITGET_HISTORY_READ_FUNNEL.md) |

| Phemex late cancel repair: final four | Source CLEAR; correctness green; direct overseer landing GO | Before retiring Phemex’s four commandless late-cancel request holdouts. | [RUST_PHEMEX_LATE_REPAIR_CONTEXT.md](RUST_PHEMEX_LATE_REPAIR_CONTEXT.md) |

| Keyless Session serial Plans | Plan and originals; source held | Before admitting a credential-free Session-authenticated serial root or changing SerialPolicy credential optionality. | [RUST_KEYLESS_SERIAL_SESSION.md](RUST_KEYLESS_SERIAL_SESSION.md) |

| RUST_BACKPACK_TRIGGER_REPAIR.md | Backpack snapshot-issued trigger repair; existing core, zero-fill replacement safety, venue-only originals/source in progress. |

| WEEX remaining inventory and identity custody | One immutable Slots owner; conservative inventory retained | Before changing WEEX role selection or claiming remaining lexical rows are raw senders. | [RUST_WEEX_REMAINING_INVENTORY.md](RUST_WEEX_REMAINING_INVENTORY.md) |

| Explicit serial call authentication | Plan; originals and source pending | Before mixing unsigned Session calls and provider-signed calls under one serial root. | [RUST_SERIAL_CALL_AUTH.md](RUST_SERIAL_CALL_AUTH.md) |

| MEXC installed signing and private custody | Signing/private-read PR; trading custody follows separately | Before changing MEXC credential setup, identity mapping, REST/WS signing or private request owners. | [RUST_MEXC_INSTALLED_SIGNING.md](RUST_MEXC_INSTALLED_SIGNING.md) |
| Lighter remaining request and signing funnel | Source reviewed; composed correctness qualification in progress | Before changing private reads, tier mutation, transaction admission, nonce retry or eager signing. | [RUST_LIGHTER_REMAINING_FUNNEL.md](RUST_LIGHTER_REMAINING_FUNNEL.md) |

| BingX managed public/private sockets | Venue-only implementation and production-binding qualification | Before changing BingX socket epochs, listen-key recovery, interests or order-read epoch publication. | [RUST_BINGX_MANAGED_SOCKETS.md](RUST_BINGX_MANAGED_SOCKETS.md) |
