# Shared exchange/types debt review

Owner codex-4ri6pb. Baseline master `f759dc8be237207558a714994da86a4266806b6d`. This reviews **123 safe-cex-exchange and 108 safe-cex-types exemption entries**, individually listed below, plus the separately maintained **89 shared-types accepted-shape debt owners**. The exemption cohort and accepted-debt ledger are different inventories.

No new reachable TypeScript divergence was established in these helper/capability declarations during this review. Keep legitimate imported capabilities, inline carriers and ownership plumbing. This is a bounded exemption/debt review, not a claim that every mapped shared algorithm or arbitrary JavaScript value now has parity. Existing manifest FINDINGs, scalar/API decisions and cancellation choices remain authoritative.

FFI, registry and transport implementation reviews remain with codex-0xekzz. Phase 2 is not on master at this baseline: request/response/body, signer-config and bridge migration is codex-dwfl8p PR #696 (latest supplied checkpoint e8cabc8b3f). Those surfaces were read for ownership classification only and are explicitly marked below; no API or body edits were made.

Native witnesses already present include exchange core_contract/core_snapshots/core_health_http_token, cancellable_future_inventory, model_field_roundtrip, native_clock, transport_binding venue integration, and types strict_wire/retained_utf16/retained_native_f64/position_record. Referenced historical witnesses are scope references, not fresh-run claims. Fresh results are recorded at the end.

## safe-cex-exchange

### `http.rs`

These declarations supply the existing adapter execution, error enhancement, telemetry/environment and response-transform call sites. Policy remains in mapped HTTP/interceptor bodies. PHASE2-OWNED: request/response/body implementation changes remain with codex-dwfl8p; no body/API parity closure is claimed here.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 039 | `HttpHost` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 040 | `HttpHost::execute` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 041 | `HttpHost::enhance_error` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 042 | `HttpHost::emit` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 043 | `HttpHost::note_response` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 044 | `HttpHost::note_success` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 045 | `HttpHost::note_failure` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 046 | `HttpHost::mark_start` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 047 | `HttpHost::signing_capture` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 048 | `HttpHost::is_development` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 049 | `HttpHost::is_main_window` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 193 | `ResponseTransform` | KEEP; PHASE2-OWNED consumer review: Native callback type for a factory-selected response transform before shared hooks. |

### `proxy_token.rs`

ProxyTokenHost names environment, virtual clock, BroadcastChannel and fetch imports for proxyTokenClient.ts. The mapped client retains token/refresh policy; host declarations do not add fallback routing. Native browser capability absence remains explicit.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 050 | `ProxyTokenHost` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 051 | `ProxyTokenHost::is_main_window` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 052 | `ProxyTokenHost::has_self` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 053 | `ProxyTokenHost::origin` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 054 | `ProxyTokenHost::virtual_clock` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 055 | `ProxyTokenHost::initialize_broadcast` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 056 | `ProxyTokenHost::broadcast` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 057 | `ProxyTokenHost::fetch` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |

### `runtime.rs`

Runtime/error/callback types retain imported host capabilities and native error identity. Date.now/performance.now are raw runtime clocks; virtualClock source readers explicitly use the shared clock at their consumers. UUID generation policy belongs to each source import (#671 fixed Coinbase); generic unique_id is not substituted for every source UUID. Error origins retain Rc cause identity and are not an invented source error class.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 058 | `ExchangeRuntime` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 059 | `ExchangeRuntime::now_ms` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 060 | `ExchangeRuntime::sleep` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 061 | `ExchangeRuntime::spawn_local` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 062 | `ExchangeRuntime::console` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 063 | `ExchangeRuntime::unique_id` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 064 | `ExchangeRuntime::is_worker` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 065 | `ExchangeRuntime::random` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 066 | `ExchangeRuntime::report_exception` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 067 | `ExchangeRuntime::execute_worker_request` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 068 | `ExchangeTask` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 069 | `ExchangeTask::cancel` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 113 | `ExchangeFuture` | KEEP: Owned local future wrapper for imported JavaScript Promise / Rust Future; browser host futures intentionally need not be Send, with no domain policy. |
| 114 | `Listener` | KEEP: Owned callback representation for the external tiny-emitter listener signature; execution and once/off behavior are mapped separately. |
| 115 | `Cleanup` | KEEP: Owned cancellation callback representation used by external runtime subscriptions; no subscription or disposal policy lives in this alias. |
| 116 | `LogSeverity` | KEEP: Exact representation of the imported @tealstreet/logging LogSeverity values; its defining external package is outside the TS extraction roots, and the enum adds no classification behavior. |
| 117 | `LogSeverity::as_str` | KEEP: Conversion of imported logging enum representation to host console severity labels; no severity classification policy. |
| 118 | `ExchangeError::new` | KEEP: Rust construction plumbing for the standard JavaScript Error boundary, with empty contextual metadata; specific error formatting and telemetry are mapped separately. |
| 171 | `ExchangeRuntime::monotonic_now_ms` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 185 | `ExchangeErrorOrigin` | KEEP: Shared native error-boundary ownership glue for existing Store/REST failures; preserves opaque typed causes and clone identity, with bounded Debug. |
| 186 | `ExchangeErrorOrigin::new` | KEEP: Native allocation glue for the shared std::error::Error origin carrier, not an SDK source method. |
| 187 | `ExchangeErrorOrigin::error` | KEEP: Read-only native std::error::Error origin accessor used by the shared Error::source implementation; no TypeScript method or venue policy. |

### `websocket.rs`

Reviewed socket/runtime/parent imports, native events/handlers, socket-presence accessor and canonical timer delegation. WsParent forwards existing exchange/Store operations; WsRuntime supplies host/library facts rather than per-venue reconnect policy. The stable RWS random minimum remains separate from SDK tuning. Existing callback/cancellation/typed-snapshot choices remain independently mapped.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 070 | `WsSocket` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 071 | `WsSocket::ready_state` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 072 | `WsSocket::next_event` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 073 | `WsSocket::send` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 074 | `WsSocket::close` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 075 | `WsSocket::terminate` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 076 | `WsSocket::reconnect` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 077 | `WsSocket::is_reconnecting` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 078 | `WsRuntime` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 079 | `WsRuntime::transport` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 080 | `WsRuntime::tunnel_host` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 081 | `WsRuntime::dial` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 082 | `WsRuntime::headers_capable` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 083 | `WsRuntime::enable_reconnecting` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 084 | `WsRuntime::min_reconnection_delay_ms` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 085 | `WsRuntime::diagnostics_enabled` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 086 | `WsRuntime::lifecycle_logs_enabled` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 087 | `WsParent` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 088 | `WsParent::name` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 089 | `WsParent::is_disposed` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 090 | `WsParent::options` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 091 | `WsParent::store` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 092 | `WsParent::is_private_socket` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 093 | `WsParent::resync_coordinator` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 094 | `WsParent::fetch_and_set_private_snapshot` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 095 | `WsParent::log` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 096 | `WsParent::emit` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 097 | `WsParent::sync_socket_health_to_store` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 098 | `WsParent::update_latency` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 099 | `WsParent::emit_ws_reconnected` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 100 | `WsParent::emit_and_log_error` | KEEP: Owned reference boundary for the websocket parent and the real shared Store; forwards venue hooks and already mapped exchange behavior without implementing an additional policy. |
| 119 | `WsEvent` | KEEP: Representation of host WebSocket events from the external socket implementation; carries open/message/close/error data and implements no state transition. |
| 122 | `MessageHandler` | KEEP: Named Rust ownership representation of the anonymous websocket callback signature; mapped subscribe/dispatch methods retain all callback behavior. |
| 130 | `WsRuntime::library_min_reconnection_delay_ms` | KEEP: Host-supplied stable random sample for vendored RWS library default minimum, distinct from SDK module tuning. |
| 169 | `BaseWebSocket::schedule_ping` | KEEP: C3 socket-host capability exposes the existing core Ping timer slot to the Apex private ping body. |
| 170 | `BaseWebSocket::has_websocket` | KEEP: Native accessor for source BaseWebSocket.ws truthiness at venue callback guards; exposes existing core-owned socket presence, including CONNECTING/CLOSED sockets, without leaking the host socket or duplicating lifecycle state. |
| 178 | `BaseWebSocket::start_interval` | KEEP: Socket-owned entry to the same shared host interval operation. |
| 179 | `BaseWebSocket::start_timeout` | KEEP: Socket-owned individually cancellable timeout wrapper replacing the need for a venue task registry. |

### `external.rs`

The external provider is an injected host dependency with typed snapshots/callbacks. Selection, health aggregation and subscription/disposal algorithms remain in BaseExchange; this interface neither invents a provider nor enables an unsupported source capability. Registry provider implementation remains the cleared owner scope.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 101 | `ExternalPublicData` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 102 | `ExternalPublicData::connect` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 103 | `ExternalPublicData::dispose` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 104 | `ExternalPublicData::is_initialized` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 105 | `ExternalPublicData::is_healthy` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 106 | `ExternalPublicData::last_message_timestamp` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 107 | `ExternalPublicData::listen_ohlcv` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 108 | `ExternalPublicData::listen_order_book` | KEEP: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |

### `signing.rs`

SigningHttpClient::capture names the existing per-request interceptor capture seam. PHASE2-OWNED: signing config/body migration stays with codex-dwfl8p. No separate signing or serialization implementation added.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 109 | `SigningHttpClient` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |
| 110 | `SigningHttpClient::capture` | KEEP; PHASE2-OWNED consumer review: Host boundary contract adapting external runtime facilities; the SDK selects policy in mapped source methods, while the host supplies I/O, scheduling, callbacks or imported service implementations. |

### `journal.rs`

JournalFolder is an imported folding operation. The wrapper still checks the branded fills axis and chooses source time window/settle currency; no second journal algorithm or invented empty result is supplied by this interface.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 111 | `JournalFolder` | KEEP: Dependency boundary for imported journal folding from packages/journaling, outside this extraction root; the mapped exchange wrapper still applies the capability and time-window policy. |
| 112 | `JournalFolder::build_folded_position_records` | KEEP: Dependency boundary for imported journal folding from packages/journaling, outside this extraction root; the mapped exchange wrapper still applies the capability and time-window policy. |

### `ws_parent.rs`

Weak parent ownership joins the actual BaseExchange options, Store, resync coordinator and venue hooks. It does not create a second socket or recovery controller. Callback-origin and disposal behavior remain the separately mapped binding contracts.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 120 | `WsExchangeParent` | KEEP: Ownership bridge from the mapped BaseExchange implementation to the host-facing WsParent reference contract; adds no routing, reconnection or state policy. |
| 121 | `WsExchangeParent::new` | KEEP: Constructs the owned parent-reference adapter for host websocket callbacks; no exchange behavior is implemented by this ownership conversion. |

### `capabilities.rs`

The sealed brand prevents callers forging a base-fallback origin. It supplies no capability implementation and does not infer support from return values.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 123 | `sealed::Sealed` | KEEP: Exact Rust host-boundary type sealing: this private-module supertrait prevents external implementations of CapabilityBrand from forging an origin check. |

### `exchange.rs`

Reviewed native self-view, applicationId/vault mutation, read-only signing operands, HTTP/WS remote request dispatch and task/timer entrypoints. Identity patches mutate the one shared options cell; signing getters read live signers/options; self-view retains concrete trait dispatch. Task accessors delegate to OwnedTasks/SharedOperation, whose cancellation classification remains independently pinned.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 127 | `ExchangeView` | KEEP: Blanket object-safe self view for Exchange default hooks invoking existing base orchestration; Rust cannot coerce an unsized Self directly to dyn Exchange. |
| 128 | `ExchangeView::as_exchange` | KEEP: Object-safe blanket self view used only to retain dynamic venue dispatch from default base-orchestration hooks; companion ABI glue to ExchangeView, no new TS behavior. |
| 131 | `Exchange::sign_remote_request` | KEEP: Exhaustive host bridge dispatches the shared transport HTTP/WS request union through the concrete venue signHttpRequest/signWsAuth hooks; no alternate signing or lifetime owner. |
| 133 | `IdentityFieldPatch` | KEEP: Narrow identity update distinguishes unchanged, present string (including empty), and removal without exposing an options borrow. |
| 134 | `AccountIdentityPatch` | KEEP: Core-owned identity patch payload for applicationId and vault; explicit operations replace venue-local parallel options copies. |
| 135 | `BaseExchange::patch_account_identity` | KEEP: Shared identity mutation operation required by venue resolvers; updates the one base/WS owner synchronously without exposing RefCell guards. |
| 175 | `BaseExchange::start_owned_operation` | KEEP: Host boundary for exchange-owned async tails outliving a caller. |
| 176 | `BaseExchange::start_interval` | KEEP: Shared host operation for Node capture-before-callback interval reinsertion and owned cancellation. |
| 177 | `BaseExchange::start_timeout` | KEEP: Shared host operation for individually cancellable exchange timers. |
| 180 | `BaseSigningOptionsView` | KEEP: Native read-only operand view used by the separately mapped shared signing getter evaluator; preserves lazy property access without adding a TypeScript declaration or an independent signing operation. |
| 181 | `BaseSigningOptionsView::key_truthy` | KEEP: Native read-only operand view used by the separately mapped shared signing getter evaluator; preserves lazy property access without adding a TypeScript declaration or an independent signing operation. |
| 182 | `BaseSigningOptionsView::secret_truthy` | KEEP: Native read-only operand view used by the separately mapped shared signing getter evaluator; preserves lazy property access without adding a TypeScript declaration or an independent signing operation. |
| 183 | `BaseSigningOptionsView::remote_signer_truthy` | KEEP: Native read-only operand view used by the separately mapped shared signing getter evaluator; preserves lazy property access without adding a TypeScript declaration or an independent signing operation. |
| 184 | `BaseSigningOptionsView::payload_signer_truthy` | KEEP: Native read-only operand view used by the separately mapped shared signing getter evaluator; preserves lazy property access without adding a TypeScript declaration or an independent signing operation. |
| 192 | `BaseExchange::start_shared_operation` | KEEP: Host bridge publishes a retained result before handing its work to the existing owned-task registry. |

### `position_intervals.rs`

FundingRow and FundingAttribution name inline source input/result shapes. attribute_funding remains the mapped arithmetic/iteration owner; these declarations contain no venue policy.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 165 | `FundingRow` | KEEP: Native name for attributeFunding's inline funding-array element shape; source declares no standalone type symbol. |
| 166 | `FundingAttribution` | KEEP: Native name for the inline funding/attributed result shared by attributeFunding and FoldPositionAdapterDeps; source declares no standalone result type. |

### `build_records.rs`

The structural exchange argument and wrapped error/cause name inline source values. build_folded_position_records remains the mapped algorithm; income-failure name/cause policy matches journal/buildRecords.ts. No venue implementation is claimed by the callable projection.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 167 | `FoldableExchange` | KEEP: Native callable projection of buildFoldedPositionRecords' structural exchange parameter; its module-private TypeScript type is not an extracted row. |
| 168 | `BuildRecordsError` | KEEP: Native rejection carrier keeps the shared ExchangeError and wrapped income cause separately; TypeScript throws errors with cause and declares no standalone named rejection type. |

### `owned_tasks.rs`

OwnedOperation exposes native cancellation/observation for the canonical task registry. The owner retains producers independently of observer drops; terminal cleanup removes registrations before reentrant cancellation. Existing explicit source lifetime choices remain in cancellation manifests.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 172 | `OwnedOperation` | KEEP: Weak host cancellation capability for owned Rust tasks. |
| 173 | `OwnedOperation::cancel` | KEEP: Host ownership operation removes the Rust registry entry before cancelling its executor handle. |
| 174 | `OwnedOperation::is_active` | KEEP: Observes the Rust owned-registration lifetime without owning or cancelling it. |
| 194 | `start_detached_timeout` | KEEP: Shared host operation for a source global timeout in an independent retained registry; explicit cancellation releases its registration while exchange disposal does not own it. |

### `shared_operation.rs`

SharedOperation is native retained-result/observer ownership over the existing task registry. Each wait registers separately; owner cancellation settles once. Its explicit Rust lifetime choices remain recorded, rather than being approved as arbitrary JavaScript Promise semantics by an exemption.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 188 | `SharedOperation` | KEEP: Rust retained-result handle over exchange-owned work. |
| 189 | `SharedOperation::wait` | KEEP: Host observer registration for a retained Rust result. |
| 190 | `SharedOperation::cancel` | KEEP: Native owner cancellation capability for retained work, analogous to the existing OwnedOperation host capability. |
| 191 | `SharedOperation::is_active` | KEEP: Observes the native owned-operation registration without adding a venue request or policy; no standalone TypeScript SDK declaration exists for this host lifetime capability. |

### `cross_trade/mod.rs`

CrossTradeLoopFn is an owned callback signature. Manager scheduling and CrossTradeExchange dispatch remain separately mapped; this alias runs no loop.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 195 | `CrossTradeLoopFn` | KEEP: Rust-only owned callback carrier for the host loop scheduler's anonymous asynchronous function argument; no standalone TypeScript declaration. |

## safe-cex-types

### `lib.rs`

Named inline object/union/literal representations remain attached to their canonical source fields. Public records use object-only front gates and enums use string front gates; private metadata companions are not public ingress. Four uninhabited capability markers reject wire substitutes; real executable signers/providers are injected at runtime. This classification does not claim widened arbitrary JS values in typed public APIs.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 001 | `MarketLimits` | KEEP: #[ts(inline)] helper for the anonymous TS object in Market.limits; adds no exported TypeScript name (C3). |
| 002 | `MarketLimitsAmount` | KEEP: #[ts(inline)] helper for the anonymous TS object in Market.limits.amount; adds no exported TypeScript name (C3). |
| 003 | `MarketLimitsLeverage` | KEEP: #[ts(inline)] helper for the anonymous TS object in Market.limits.leverage; adds no exported TypeScript name (C3). |
| 004 | `MarketPrecision` | KEEP: #[ts(inline)] helper for the anonymous TS object in Market.precision; adds no exported TypeScript name (C3). |
| 005 | `OrderExtra` | KEEP: #[ts(inline)] helper for the anonymous TS object in Order.extra; adds no exported TypeScript name (C3). |
| 006 | `PositionExtra` | KEEP: #[ts(inline)] helper for the anonymous TS object in Position.extra; adds no exported TypeScript name (C3). |
| 007 | `ExecutionPositionSide` | KEEP: Inline representation of Execution.positionSide, HistoricalTrade.positionSide and HistoricalOrder.positionSide: preserves PositionSide enum identity plus the literal both; adds no exported TypeScript name. |
| 008 | `BothPositionSide` | KEEP: Singleton literal both for the inline ExecutionPositionSide union; no TypeScript runtime enum or behavior is added. |
| 009 | `ExchangeAccountConfigBybitSiteId` | KEEP: C3 inline representation of the anonymous TS field ExchangeAccountConfig.bybitSiteId; adds no exported TS name or behavior. |
| 010 | `ExchangeAccountConfigHealthStatus` | KEEP: C3 inline representation of the anonymous TS field ExchangeAccountConfig.healthStatus; adds no exported TS name or behavior. |
| 011 | `ExchangeOptionsAuth` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.auth; adds no exported TS name or behavior. |
| 012 | `ExchangeOptionsHyperliquidBuilder` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.hyperliquidBuilder; adds no exported TS name or behavior. |
| 013 | `ExchangeOptionsBybitSiteId` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.bybitSiteId; adds no exported TS name or behavior. |
| 014 | `ExchangeOptionsTuning` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.tuning; adds no exported TS name or behavior. |
| 015 | `ExchangeOptionsExtra` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.extra; adds no exported TS name or behavior. |
| 016 | `ExchangeOptionsExtraAsterdexApiVersion` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.extra.asterdexApiVersion; adds no exported TS name or behavior. |
| 017 | `ExchangeOptionsExtraLighterBuilderFeeMode` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.extra.lighterBuilderFeeMode; adds no exported TS name or behavior. |
| 018 | `ExchangeOptionsExtraKucoinBroker` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.extra.kucoinBroker; adds no exported TS name or behavior. |
| 019 | `ExchangeOptionsExternalPublicData` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.externalPublicData; adds no exported TS name or behavior. |
| 020 | `ExchangeOptionsExternalPublicDataVariant0` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.externalPublicData (websocket variant); adds no exported TS name or behavior. |
| 021 | `ExchangeOptionsExternalPublicDataVariant0Type` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.externalPublicData (websocket variant).type; adds no exported TS name or behavior. |
| 022 | `ExchangeOptionsExternalPublicDataVariant1` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.externalPublicData (in-process variant); adds no exported TS name or behavior. |
| 023 | `ExchangeOptionsExternalPublicDataVariant1Type` | KEEP: C3 inline representation of the anonymous TS field ExchangeOptions.externalPublicData (in-process variant).type; adds no exported TS name or behavior. |
| 024 | `UpdateOrderOptsUpdate` | KEEP: C3 inline representation of the anonymous TS field UpdateOrderOpts.update; adds no exported TS name or behavior. |
| 025 | `UpdateOrderOptsUpdateVariant0` | KEEP: C3 inline representation of the anonymous TS field UpdateOrderOpts.update (amount variant); adds no exported TS name or behavior. |
| 026 | `UpdateOrderOptsUpdateVariant1` | KEEP: C3 inline representation of the anonymous TS field UpdateOrderOpts.update (price variant); adds no exported TS name or behavior. |
| 027 | `ClosedPnlEntryNotional` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.notional; adds no exported TS name or behavior. |
| 028 | `ClosedPnlEntryEntryPrice` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.entryPrice; adds no exported TS name or behavior. |
| 029 | `ClosedPnlEntryExitPrice` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.exitPrice; adds no exported TS name or behavior. |
| 030 | `ClosedPnlEntryUnrealizedPnl` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.unrealizedPnl; adds no exported TS name or behavior. |
| 031 | `ClosedPnlEntryFees` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.fees; adds no exported TS name or behavior. |
| 032 | `ClosedPnlEntryContracts` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.contracts; adds no exported TS name or behavior. |
| 033 | `ClosedPnlEntryLeverage` | KEEP: C3 inline representation of the anonymous TS field ClosedPnlEntry.leverage; adds no exported TS name or behavior. |
| 034 | `True` | KEEP: Singleton boolean true used by PublicExchangeOptions.publicDataOnly; preserves the TS literal and has no exported TS declaration or new behavior. |
| 035 | `RemoteSignerCapability` | KEEP: C4 uninhabited marker for the existing TS runtime capability; distinct Rust type rejects present wire values and implements no invocation or JSON substitute. |
| 036 | `PayloadSignerCapability` | KEEP: C4 uninhabited marker for the existing TS runtime capability; distinct Rust type rejects present wire values and implements no invocation or JSON substitute. |
| 037 | `TransportCapability` | KEEP: C4 uninhabited marker for the existing TS runtime capability; distinct Rust type rejects present wire values and implements no invocation or JSON substitute. |
| 038 | `PublicDataSourceCapability` | KEEP: C4 uninhabited marker for the existing TS runtime capability; distinct Rust type rejects present wire values and implements no invocation or JSON substitute. |

### `js_coercion.rs`

One lowest-layer implementation supplies JSON-domain String conversion, ryu-js number spelling, integer-index own-key order and UTF-16 string replacement/quotation. Callers retain their source nullish/truthy branch before coercion. JSON objects with non-callable own toString remain fallible. Prototype mutation and arbitrary callable/accessor inputs are not added by these helpers.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 124 | `JsCoercionError` | KEEP: Named Rust Result error for source TypeError during JSON String(any) coercion; shared host-boundary glue, no new TS behavior. |
| 125 | `js_string` | KEEP: Shared String(any) ABI glue requested for measured venue field coercions; JSON objects with non-callable own toString remain fallible, no new TS behavior. |
| 126 | `js_number_string` | KEEP: Lowest-layer shared Number-to-String backend for JSON String(any); num's canonical formatter delegates here because model utilities cannot depend upward on num. |
| 132 | `js_string_replace_utf16` | KEEP: Lowest-layer UTF-16 replacement backend for the JS String.replace/replaceAll intrinsic; model helpers delegate and num supplies public wrappers plus the maintained live oracle. |
| 137 | `js_array_index` | KEEP: Lowest-layer shared own-property ordering backend for recursive Balance currencies/coins; num delegates here to avoid a types-to-num dependency cycle. |
| 138 | `js_own_keys` | KEEP: Lowest-layer shared own-property ordering backend for recursive Balance currencies/coins; num delegates here to avoid a types-to-num dependency cycle. |
| 139 | `normalize_object_order` | KEEP: Lowest-layer shared own-property ordering backend for recursive Balance currencies/coins; num delegates here to avoid a types-to-num dependency cycle. |
| 140 | `normalize_json_order` | KEEP: Lowest-layer shared own-property ordering backend for recursive Balance currencies/coins; num delegates here to avoid a types-to-num dependency cycle. |
| 230 | `json_quote_utf16` | KEEP: C3: shared ECMAScript JSON string quotation primitive used by the retained writer and runtime JSON.stringify; preserves lone UTF-16 units as escapes. |
| 231 | `js_own_keys_utf16` | KEEP: C3: UTF-16 key adapter to the existing ECMAScript array-index classifier; native key enumeration keeps integer indices ascending and ordinary names in insertion order. |

### `strict_decode.rs`

The object front gate invokes deserialize_map before private derived record decoders. It rejects positional record arrays while leaving tuple protocols separately declared. Existing public-qualified/nested record witnesses cover the real entrypoints.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 129 | `object` | KEEP: Shared record-shape guard for TypeScript object contracts: forces deserialize_map before private serde-derived companions, preserving streaming field decoding and rejecting positional arrays. |

### `object_map.rs`

JsObjectMap privately owns insertion order and reorders canonical integer keys. Replacement, deletion/reinsertion and get_mut preserve the intended own-property policy; callers cannot mutate backing map through DerefMut. Prototype-changing __proto__ operations retain their existing declared refusal/choice.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 136 | `JsObjectMap` | KEEP: Lowest-layer shared own-property ordering backend for recursive Balance currencies/coins; num delegates here to avoid a types-to-num dependency cycle. |
| 141 | `JsObjectMap::new` | KEEP: Ordered Balance dictionary adapter operation; private backing map ensures mutations cannot bypass OrdinaryOwnPropertyKeys ordering. |
| 142 | `JsObjectMap::insert` | KEEP: Ordered Balance dictionary adapter operation; private backing map ensures mutations cannot bypass OrdinaryOwnPropertyKeys ordering. |
| 143 | `JsObjectMap::remove` | KEEP: Ordered Balance dictionary adapter operation; private backing map ensures mutations cannot bypass OrdinaryOwnPropertyKeys ordering. |
| 144 | `JsObjectMap::get_mut` | KEEP: Ordered Balance dictionary adapter operation; private backing map ensures mutations cannot bypass OrdinaryOwnPropertyKeys ordering. |

### `position_record.rs`

PositionRecordLeg and PositionRecordState name inline unions in the canonical journal record. Their string-front decoders and explicit spellings remain canonical; they introduce no folding or venue adapter.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 145 | `PositionRecordLeg` | KEEP: Named Rust wrapper for the inline positionSide or state closed union in canonical PositionRecord; accepted spellings inventoried against the actual TypeScript declaration. |
| 146 | `PositionRecordState` | KEEP: Named Rust wrapper for the inline positionSide or state closed union in canonical PositionRecord; accepted spellings inventoried against the actual TypeScript declaration. |

### `subscriber_error.rs`

Bottom-layer callback aliases and error/cause carrier preserve native subscriber cause identity without a dependency cycle. Consumers still own commit-before-error, stopping fanout and catch policy; presence of this carrier alone does not close notification findings in venue manifests.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 147 | `StoreSubscriberError` | KEEP: Catchable host subscriber failure and mutation-origin representation; shared fallible fanout stops on first error after commit and leaves caller recovery policy explicit. |
| 148 | `StoreSubscriberError::new` | KEEP: Catchable host subscriber failure and mutation-origin representation; shared fallible fanout stops on first error after commit and leaves caller recovery policy explicit. |
| 149 | `StoreSubscriberError::with_response` | KEEP: Catchable host subscriber failure and mutation-origin representation; shared fallible fanout stops on first error after commit and leaves caller recovery policy explicit. |
| 150 | `StoreSubscriberError::message` | KEEP: Catchable host subscriber failure and mutation-origin representation; shared fallible fanout stops on first error after commit and leaves caller recovery policy explicit. |
| 151 | `StoreSubscriberError::response` | KEEP: Catchable host subscriber failure and mutation-origin representation; shared fallible fanout stops on first error after commit and leaves caller recovery policy explicit. |
| 152 | `StoreSubscriber` | KEEP: Bottom-layer callback alias shares fallible notification semantics between Store and its public-data view without a dependency cycle or an infallible compatibility path. |
| 153 | `PublicDataSubscriber` | KEEP: Bottom-layer callback alias shares fallible notification semantics between Store and its public-data view without a dependency cycle or an infallible compatibility path. |
| 154 | `StoreSubscriberError::with_cause` | KEEP: Host callback error retains opaque original cause identity for consumer downcasting; Rust error plumbing has no standalone TS declaration. |

### `retained_json.rs`

Canonical iterative JSON parser/capture retains UTF-16 keys/text, numeric lexemes/native bits and info presence. Value projection remains fallible for lone units/NaN. PHASE2-OWNED: direct request/response consumers stay with dwfl8p; retaining these primitives does not approve every old JSON-copy consumer.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 155 | `RetainedJsonInput` | KEEP; PHASE2-OWNED consumer review: Canonical lower-layer retained JSON capture extracted from numeric ingress; preserves lexemes and info presence without a types-to-num cycle, no standalone TypeScript declaration. |
| 156 | `RetainedJsonInput::from_value` | KEEP; PHASE2-OWNED consumer review: Canonical lower-layer retained JSON capture extracted from numeric ingress; preserves lexemes and info presence without a types-to-num cycle, no standalone TypeScript declaration. |
| 157 | `RetainedJsonInput::into_value` | KEEP; PHASE2-OWNED consumer review: Canonical lower-layer retained JSON capture extracted from numeric ingress; preserves lexemes and info presence without a types-to-num cycle, no standalone TypeScript declaration. |
| 158 | `deserialize_retained_json` | KEEP; PHASE2-OWNED consumer review: Canonical lower-layer retained JSON capture extracted from numeric ingress; preserves lexemes and info presence without a types-to-num cycle, no standalone TypeScript declaration. |
| 159 | `deserialize_present_retained_json` | KEEP; PHASE2-OWNED consumer review: Canonical lower-layer retained JSON capture extracted from numeric ingress; preserves lexemes and info presence without a types-to-num cycle, no standalone TypeScript declaration. |
| 229 | `RetainedJsonInput::parse_json` | KEEP; PHASE2-OWNED consumer review: C3: shared ECMAScript JSON.parse data primitive; one iterative grammar preserves UTF-16 strings/keys and native binary64 numbers for runtime and transport consumers. |

### `validation_error.rs`

Runtime account validation errors retain JSON values without String coercion. The source static string declaration and finite owned JSON representation remain separately classified. No arbitrary JavaScript thrown-value/transport widening is claimed.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 160 | `AccountValidationError` | KEEP: Shared runtime validation error carrier preserves source non-string JSON values without coercion; static declaration projection is recorded on ValidateAccountResult. |
| 161 | `AccountValidationError::from_value` | KEEP: Shared runtime validation error carrier preserves source non-string JSON values without coercion; static declaration projection is recorded on ValidateAccountResult. |
| 162 | `AccountValidationError::as_value` | KEEP: Shared runtime validation error carrier preserves source non-string JSON values without coercion; static declaration projection is recorded on ValidateAccountResult. |
| 163 | `AccountValidationError::into_value` | KEEP: Shared runtime validation error carrier preserves source non-string JSON values without coercion; static declaration projection is recorded on ValidateAccountResult. |

### `query_value.rs`

QueryValue intentionally keeps undefined and non-finite native numbers separate from JSON. Rendering belongs to the selected transport serializer; this carrier has no Serialize or implicit Value conversion. No body/transport changes made.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 164 | `QueryValue` | KEEP: Shared non-JSON query carrier retains undefined and nonfinite numeric values until transport-selected rendering; no standalone source declaration or JSON projection. |

### `model_field.rs`

Absent/Undefined/Value is the canonical model-presence carrier. Read helpers intentionally expose undefined as an absent read; into_property and serialization retain own-property semantics. Patch policy belongs to Store merge consumers, not Option-like read helpers. No public PlaceOrderOpts widening is implied. ModelField native visitor coverage landed in #688; prepared amendment inputs remain separate.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 196 | `MODEL_FIELD` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 197 | `MODEL_RECORD` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 198 | `ModelField` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 199 | `ModelField::and_then` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 200 | `ModelField::as_deref` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 201 | `ModelField::as_field_ref` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 202 | `ModelField::as_mut` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 203 | `ModelField::as_ref` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 204 | `ModelField::expect` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 205 | `ModelField::filter` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 206 | `ModelField::into_property` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 207 | `ModelField::into_value` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 208 | `ModelField::is_absent` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 209 | `ModelField::is_none` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 210 | `ModelField::is_some` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 211 | `ModelField::is_some_and` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 212 | `ModelField::is_undefined` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 213 | `ModelField::map` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 214 | `ModelField::ok_or_else` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 215 | `ModelField::or` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 216 | `ModelField::or_else` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 217 | `ModelField::serialize_field` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 218 | `ModelField::take` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 219 | `ModelField::to_number` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 220 | `ModelField::unwrap` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 221 | `ModelField::unwrap_or` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 222 | `ModelField::unwrap_or_default` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 223 | `ModelField::unwrap_or_else` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 224 | `ModelField::value` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 225 | `UNDEFINED` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 226 | `deserialize_info` | KEEP: Shared model presence codec plumbing: distinguishes an absent property, an own undefined property, and a typed value for Order/Position. |
| 228 | `ModelField::get_or_insert_with` | KEEP: Shared model presence mutation used by the webworker Order.extra postprocessor: reads absent and undefined as undefined, and creates a present value when the source initializes that property. |

### `prepared_order.rs`

PreparedOrderOpts copies runtime amendment type/side presence into a dedicated internal carrier. Public PlaceOrderOpts and validated write tokens remain strict. Existing native ModelField/PreparedOrder witnesses cover presence/numbers before JSON encoding.

| Entry | Declaration | Disposition / declaration role |
|---:|---|---|
| 227 | `PreparedOrderOpts` | KEEP: Internal prepared placement input representation for mapped amendment paths copying Order.type/side, which can be undefined at runtime. |

## Historical accepted-shape debt: 89 types owners

The canonical outstanding-defects.json is removal-only. These fingerprints were measured at its historical base, and must not be reinterpreted as production reachability findings. Current public string/map guards already reject the old external-tag/positional-array forms. Both native and WASM compiler-owned probes pass on baseline master f759dc8be2: 212 runtime results and 30 classifications per configuration, with zero failed owners. Every one of these 89 types has an actual runtime receipt in both configurations. The scoped real ratchet rejects the unchanged debt as stale and passes after removal. This PR removes only these 89 rows / 1004 fingerprints; every other crate row remains byte-equivalent as a parsed value. No Rust type/API or decoder changes are needed.

| Owner | Historical fingerprints | Current review |
|---|---:|---|
| `safe_cex_types::AnyTimeframe` | 108 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Balance` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::BothPositionSide` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Candle` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ClosedPnlEntry` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::CoinBalance` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::DataProvenance` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::DisplayedMargin` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeAccount` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeAccountConfig` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeAccountConfigBybitSiteId` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeAccountConfigHealthStatus` | 20 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeName` | 188 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsAuth` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsBybitSiteId` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExternalPublicData` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExternalPublicDataVariant0` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExternalPublicDataVariant0Type` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExternalPublicDataVariant1Type` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExtra` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExtraAsterdexApiVersion` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExtraKucoinBroker` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsExtraLighterBuilderFeeMode` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsHyperliquidBuilder` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExchangeOptionsTuning` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Execution` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExecutionPositionSide` | 12 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExecutionsOptions` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ExtendedTimeframe` | 44 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::FetchedLeverage` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::CandleChannel` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::DataTag` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayCandleUpdate` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayChannel` | 28 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayMarketUpdate` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayMessage` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayMessageType` | 32 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayOrderbookUpdate` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayTickerData` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::GatewayTickerUpdate` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::MarketChannel` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::OrderbookChannel` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::gateway::TickerChannel` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::HistoricalOrder` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::HistoricalTrade` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::HistoricalTradeOptions` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::IncomeCategory` | 28 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::IncomeHistoryOptions` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::IncomeRecord` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MarginType` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Market` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MarketLimits` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MarketLimitsAmount` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MarketLimitsLeverage` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MarketPrecision` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::MessageHandlerStatsType` | 16 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OHLCVOptions` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OHLCVSOptions` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Order` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderBook` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderBookOrders` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderExtra` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderHistoryOptions` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderRejectReason` | 12 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderRiskProfile` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderSide` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderStatus` | 24 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderStopTriggerType` | 12 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderTimeInForce` | 16 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::OrderType` | 24 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::PlaceOrderOpts` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Position` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::PositionExtra` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::PositionMode` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::PositionSide` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::PublicTrade` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::RequestOutcome` | 20 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::RequestOutcomeCounts` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::SecondsTimeframe` | 16 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::StoreDataLoaded` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::StoreOptions` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::StoreUpdateIds` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Ticker` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::Timeframe` | 48 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::UpdateOrderOpts` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::UpdateOrderOptsUpdate` | 8 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::UpdateOrderOptsUpdateVariant0` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::UpdateOrderOptsUpdateVariant1` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |
| `safe_cex_types::ValidateAccountResult` | 4 | ALREADY-ON-MASTER `f759dc8be2`: native + WASM runtime receipts have zero failures; stale debt retired in this PR. |

## Fresh validation

- `node sdk-rs/safe-cex-parity/scripts/accepted-set.mjs --measure --package=safe-cex-types --jobs=1`: PASS, 151 compiler-confirmed Deserialize types, 70 alternate-only owners, 212 runtime results / 30 classifications, zero failures.
- Same scoped command with `--target=wasm32-unknown-unknown`: PASS, same owner counts, zero failures.
- Verified all 89 historical owners have runtime (not classification-only) receipts on both targets. Old types-only debt fails `checkRatchet` with “repaired outstanding defect is still listed”; empty types debt passes with zero failures. This is a ledger correction after existing production fixes, not a new venue algorithm.
- Types declares no feature matrix; both native and WASM configurations were measured. No workspace suite or broad matrix was run.
- All 231 exemption declarations remain: 123 exchange and 108 types. Per-entry census is unique; no exemption was deleted to silence a gate.
- Phase 2 body/transport/signing API implementations remain untouched. Existing mapped FINDINGs and shared representation decisions remain open under their own owners.

Scoped ratchet logs and exact native/WASM receipts are preserved as `/tmp/port-txflow-types-debt-ratchet.log`, `/tmp/port-txflow-types-native-results.json` and `/tmp/port-txflow-types-wasm-results.json` in this worktree session. The maintained accepted-set runner remains the executable authority; these historical receipts are not replacement fixtures.

## Publication recheck (2026-10-18 SGT)

Current master `bc706182c181eeaf34eaeacf122f62da12143eb6` was normally merged before publication. The exact
123 exchange +108 types exemption rows remain unchanged from `f759dc8be2`.
All26 Rust source files containing those declarations are byte-identical to the
reviewed baseline, and the ledger has exactly one row for every current entry.
Other mapped runtime implementations have changed independently; their source
semantics are not closed by these exemption classifications.

The complete safe-cex-types source tree, package manifest, locked dependencies,
toolchain pin and maintained TS types declaration are also unchanged. The earlier
native/WASM runtime measurements therefore observe the same production decoders.
Rechecked every89 retired owner against an actual runtime receipt on both targets:
212 runtime results +30 explicit classifications per target, zero failures. The
unchanged production ratchet rejects the old89-owner/1004-fingerprint types debt
and accepts its removal. Production history validation is removal-only, and the
remaining9 owners/28 fingerprints are byte-for-byte unchanged from current master.

Phase2 #696 remains open at this publication check (checked head: fc987325b9e0a18aa2faa937f60aa9ac7475a310). Its marked body/transport consumers remain owned/deferred;
this PR changes no body, model, API, exemption or scalar representation decision.
All final shared-type decisions remain under their designated implementation
owners; they are not implied implemented by this metadata cleanup.

Publication gates after that merge:

- Production accepted-set ratchet: old types debt RED, its removal GREEN on both native and WASM receipts; removal-only ancestry validation PASS.
- Full-resolution scoped parity: PASS, 7,459 rows / 813 exchange public items and 7,459 rows / 253 types public items.
- Required all-features cancellation inventory (`--jobs 1`): PASS, six tests and one intentional discovery-export ignore.
- Extractor dependency and global extraction checks: PASS, zero manifest changes or added/orphaned symbols, after normal-merging master with #759's Lighter manifest repair. Neither PR file changes extraction inputs.
