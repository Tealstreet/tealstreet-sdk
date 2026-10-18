# Aster runtime divergence audit

The constructed `AsterDEXExchange` tests in `src/fix_audit_tests.rs` use the real
NativeReplayRuntime, canonical Store, API signing/interceptors, clock and private
repair owners. Only the terminal HTTP host supplies responses and held requests.
The source oracle instantiates the maintained exchange/private socket classes.

| Assigned row | Result and evidence |
| --- | --- |
| syncClientServerTime | FIX: disposal previously stranded sync/start/force-sync waiters and left syncInFlight set. The compiled native test fails at ASTER-CLOCK-SETTLEMENT before the change. One common completion boundary now uses shared start_eager; held work settles, clears the slot and cannot write a stopped generation or install a late interval. |
| syncPositionMode | FIX in #1115: observable Axios error fields survive the existing owned origin. Original object identity remains the approved decision-2 residual. The native error tests execute that actual method and both maintained classes. |
| startOpenInterestPolling | ACCEPT: the real binding sends immediately, commits openInterest and repeats at 15000ms. Disposal suppresses late effects and further cycles under the overseer's teardown ruling. |
| checkAsterBuilderApproved | ACCEPT at the constructed native boundary: neither the receiver nor installed globals exposes this browser export. The previously authorized missing ApprovalHost/browser-wallet binding remains explicit in known-residuals.md; fake wallet tests do not prove that missing host. |
| runPositionRepair | ACCEPT: actual signed fetch remains held with busy=true; disposal releases busy and prevents late Store/loaded writes, as authorized. |
| updateOrder | The old missing suppression metadata is already fixed by #468 (3f2919767b). The real exchange_trait cancellation regressions cover both 200 and 400 unknown-order replies; the all_sixteen_defaults_are_bound_through_the_constructed_exchange checks DELETE before replacement POST. Required typed argument admission remains decision 5. |
| AsterDEXPrivateWebsocket | ACCEPT: the constructed binding owns the actual private socket and repair/listen-key owners. Existing NativeRuntime private balance/position frame tests assert canonical Store plus account/positions events. Host/object identity is the explicit native ownership boundary, rather than a second socket implementation. |
| generateAsterDEXCodeMessageSignature | ACCEPT by explicit security ruling: the same TypeError class is preserved while the secret is redacted. Maintained ethers source demonstrably includes the fixture key; the native binding test requires its absence. |
| scheduleListenKeyRefresh | ACCEPT: the source's live keepalive paths supply fixed numeric delays; the real binding installs a refresh timer, clears it on dispose and sends no late request. Arbitrary setTimeout argument coercion is not an input to that native method family. |
| startListenKeyWatchdog | ACCEPT: actual listen-key acquisition starts the watchdog; source and native dispose clear it. The probe advances beyond the 70-minute stale threshold after disposal and requires no HTTP or new timer work. |
| scheduleBalanceReconcile | ACCEPT: actual account fetch starts at 1000ms, never 999ms; disposal clears the timer and pending work under the teardown ruling. |
| scheduleOrderReconcile | ACCEPT: the native replacement at 5000ms fires at 15000ms, never the replaced 10000ms deadline. Disposal clears it. |

The inherited console host is an infallible native terminal capability; replacing
a JavaScript global console function is not exposed by the constructed binding.
This audit does not claim a programmable console host or a completed browser
approval entrypoint.

## Additional factory lead

The actual Aster syncPositionMode and WOO client source wrap a rejected scalar in
Error, mark isAxiosError, retain originalError, attach an ISO context timestamp
and expose an enhancedMessage-backed message getter. The native factory previously
returned the scalar unchanged. The new compiled Aster witness fails with "bare
rejection" before the factory change and passes with the source error shape.

The shared native factory applies the enhancer before venue handlers, retaining
Error class/metadata, native UTF-16 and source field-read order. The existing
OwnedCallbackError snapshot carries its fields across the Send+Sync REST boundary;
its private self marker reconstructs originalError===error without serializing a
cycle or adding another origin carrier. Original cross-boundary object identity
remains decision 2. The plain underlying Axios client keeps its scalar rejection
contract. WOO supplies an independent constructed-client/typed-bridge witness.
