# Selected delegated signer mode

Plan, 2026-10-23. Fresh branch from actual landing `90dc6637d6e12eafecc283d0dfb9a78e7fc967d6`. No OKX or socket draft is an ancestor. Owner queue: immediately after actual #1741, then #1773 consumes this actual fix.
Shared setup bug: raw callback-presence intent currently drives private PayloadSigner rejection and the native payloadSigner flag, although the shared provider selects RemoteSigner first. With both callbacks, this wrongly refuses Hyro (and the proposed OKX port) or suppresses Binance/Bybit streaming under REST-only feed policy.

Add one exported pure selector in delegatedSignerSetup.ts: remote when remoteSigner is present, otherwise payload when payloadSigner is present, otherwise undefined. Keep raw intent/publicMode unchanged and existing strict callback shape validation. Expose usesPayloadSigner on the setup from this selector; preserve raw publicMode construction fields (including rust.exchange.ts publicMode.payloadSigner) and use selected mode only for the actual top-level native boot flag; construct native payloadSigner from this selected mode. RustEngineExchange uses the same selector for setup selection/unsupported-PayloadSigner admission, never a venue branch or local fallback. No external wire/schema/secret ownership/HTTP or socket mechanism changes.

Original-RED controls for all 9 aliases (Binance4 + Bybit5 including Hyro): both callbacks -> native payloadSigner:false, raw publicMode both:true, RemoteSigner only invoked, no PayloadSigner/local fallback; payload-only remains native true and REST-only for the 8 admitted aliases; Hyro stays typed unsupported. Dedicated Hyro constructor both-callback acceptance versus PayloadSigner-only typed unsupported (with and without raw local secret). Missing/unavailable/wrong-ID RemoteSigner must not fall back. Construct fake engine to inspect actual setup output, no network/WASM requirement for this narrow RED; add actual-WASM Hyro native feed smoke only if existing fixture supports it without new source scope.

Verify focused shared setup/provider/constructor tests, SDK typecheck/lint and independent correctness/adversarial reviews; same PR caller/plan/index docs. New small shared PR queues immediately after1741;1773 consumes actual landing rather than carrying the fix. No merge under1741 hold.

Architecture gate: correct the central mode-selection choke point for existing Binance/Bybit/Hyro; not an OKX-specific bypass. Raw public intent remains separate from selected runtime mode.

Original controls reproduce 20 assertion failures and retain 14 compatibility
passes across nine aliases, including dedicated Hyro admission. The shared
selector, selected native setup flag and SDK admission/boot cache now pass all
34 controls. Existing setup/runtime expectations are corrected only for the
native flag; their raw public intent and mutation/forged-input checks remain.
Six focused SDK suites pass 129 tests; SDK typecheck and lint pass. Both
correctness and adversarial reviews are CLEAR. Actual dependency integration
and final CI/proof remain pending. No native/engine/venue source or external schema changes.

Actual RequestContext #1741 `b2fe97324763e1e2702c69ac5b839da37102928b`
is genuinely merged at `2c00b2ce3e9420cadb53838765dbe6bee20b1dd1`.
Two independent preservation reviews confirm exact eight-file own scope,
all prior owned source/test/caller blobs unchanged, the complete unowned tree
matching actual #1741, and both README rows retained once. Incoming managed
HTTP, request lifecycle and provider interfaces remain unchanged. Post-integration
focused SDK checks, typecheck and lint pass; final CI and authenticated replay
comparison use this integrated source and the actual #1741 baseline.
