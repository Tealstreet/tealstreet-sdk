# Public TS recorder support

Status: plan before implementation. Fresh branch from actual Orderly ACK merge
#1704 (`74c45b4f1628`), including actual fee #1701 and browser warnings #1703.
No live connection or new capture is authorized or run in this phase.

## Scope and evidence boundary

The existing `record.mjs` constructs a Rust EngineHost. Its traffic cannot prove
coverage of subscriptions made only by TS. Add an opt-in TS recorder without
changing that Rust recorder, existing fixtures, replay/entropy/oracles, production
or comparator/gate policy. The first CLI support is deliberately **Binance USD-M
public TS only**, whose actual constructors create clients/sockets without I/O;
standard HTTP Axios adapters and native global/Node sockets are auditable here.
Generic capture mechanics are injectable and tested offline. Other catalog
entries, especially Orderly's test-reference TS classes, remain unsupported until
their transport/constructor path is audited. This does not certify all venues,
actual TS-only channel coverage, paired oracles, live captures or private parity.

Files: new `sdk-rs/harness/record-ts.mjs`, `record-ts-runtime.mjs` and
`record-ts.test.mjs`; this plan + README index; truthful recording-limit entries
in RUST_AUDITS and RUST_ARCHITECTURE_GATE_AUDIT. No package/workflow change.
Run via `corepack yarn -s node --import tsx sdk-rs/harness/record-ts.mjs ...`.

## Implementation

1. CLI accepts only venue/seconds/books/out/testnet. Validate bounded positive
   duration/startup/body/total-output limits, distinct requested symbols, audited
   production catalog path and public options BEFORE dependency loading or I/O.
   Reject ambient HTTP/HTTPS/ALL proxy settings and force Axios proxy:false;
   direct recording must not silently inherit another route.
   No options-env. Reject credentials (including account/vault), session venues,
   remote signing, externalPublicData, custom transport/routing/endpoints and
   reference modules. Fixed publicDataOnly, no tunnel/proxy. No RNG/clock replay
   injection, initialization signer, oracle reconstruction or synthetic sends.
2. Runtime takes injectable loader, clocks/timers and sink for offline tests.
   Install global WebSocket/fetch and header-capable Node `ws` wrappers BEFORE
   dynamically importing the production TS class. Preserve Node exports/static
   WebSocket alias/send overloads/callbacks. Observe actual open/message/close
   and ALL actual outgoing data sends, including ordinary TS-only subscriptions;
   preserve text/binary and canonical-URL socket/reconnect ordinals. Incoming
   Node Buffer text is text when isBinary=false. Reject unsupported payloads;
   capture errors fail the recording rather than escaping event handlers.
   Serialize bounded asynchronous binary/Blob conversion so recorded event order
   survives slow conversions and cleanup waits for that queue.
3. Construct the real DefaultStore/TS Binance class using fixed public options.
   Wrap every axiosClients() client BEFORE start, verify its adapter resolves to
   the standard HTTP adapter, delegate that actual adapter, and record raw
   buffered responses before transformResponse. Use Axios getUri for its actual
   paramsSerializer and transformed request body. Retain error-status responses,
   cancellation and ordinary transforms. A request interceptor rejects any
   request-level adapter override. Remote-signing inner slots/custom adapters,
   Axios fetch adapters and responseType=stream are explicitly unsupported;
   reject rather than miss or double-record traffic. Assign HTTP ordinals at
   dispatch, independent of reversed response completion.
4. Direct fetch is separately observed using bounded finite response capture;
   preserve original response/body delivery via a clone and caller cancellation.
   Require a bounded supported request body/finite response, track capture EOF;
   truncated/aborted/oversize/invalid UTF-8 or unsupported streams fail closed.
   Axios fetch delegation is rejected before dispatch, avoiding duplicate
   captures. Restore globals/module cache/adapter descriptors and interceptors.
5. Reuse public recording policy/timeline and real readiness: markets loaded,
   venue.tsBooksReady if present, distinct requested symbols observed through
   actual listenOrderBook callbacks, plus public HTTP/frame activity. A repeated
   callback cannot replace a missing requested book. Start the timed window only
   after readiness. Startup/finish are bounded. Metadata labels exact TS source;
   no fake or rebuilt subscription payload is generated.
6. On finish stop new dispatch, unsubscribe/dispose exchange, close sockets,
   cancel or drain pending HTTP/capture operations under a bounded shutdown,
   restore all hooks in finally. Do not publish incomplete recordings. Exclusive
   temporary output, bounded per-record/total bytes, full writes/fsync/close,
   then atomic no-overwrite publication (same-directory hard link). Remove temp
   on failure; no partial file may look like a complete tape. CLI process
   isolation contains imported runtime timers; unit tests verify owned cleanup.

## Verification and audit

- Offline fake Exchange demonstrates TS-only startup/book sends that a fake
  Rust traffic source lacks. This proves recorder mechanics only, not a missing
  live channel actually captured. No real venue network in tests or this phase.
- Actual Axios serializer/transforms and raw large-ID strings; reversed dispatch
  completions; recorded error status; adapter override/custom/fetch/stream refusal.
- Global/Node sockets, startup sends, send callbacks, text Buffer vs binary,
  reconnect ordinals; direct fetch EOF/cancellation/limits/consumer preservation.
- Credential/account/reference/external-provider rejection BEFORE loader/I/O;
  unique books, duplicate callbacks/missing books, polling readiness semantics;
  startup errors and delayed/incomplete operations; sink failure/overwrite/atomic
  publication; all hooks/resources restored on success and failure.
- Independent plan and adversarial code reviews, focused tests, serial harness
  and root typecheck/lint; full PR-head CI (existing replays unchanged), fresh
  separate dependency guard/merge, actual tree/parents audit and Aimux report.

## Remaining owner evidence

Actual TS-origin public recaptures (including all unsupported transport families),
all-venue private captures, 3–7-day shadow and canary need approved live recording
and owner evidence. Synthetic Hyro/Breakout/Bitget-USDC provenance stays explicit.
Native live Fill timestamps/optional execution IDs need native provenance and a
separately reviewed contract. The bounded ts-oracle/fixer-contracts/legacy-fill
review found no additional demonstrated circular/overbroad defect; retain the
executable TS references and reconstruction caveats. Combined/family/browser
labels remain distinct. No production cleanup or threshold change is claimed.

## Implementation refinements and current evidence

The plan was independently reviewed and committed as `21bd3346148` before code.
The actual production Binance constructor is exercised offline with all network
attempts forbidden; its owned clients resolve to standard HTTP adapters. The
loader also refuses a pre-existing non-direct TS transport. Metadata source is
`live-ts-public`; none of the fake test output is a committed capture fixture.

The HTTP delegate requests arraybuffer bytes to avoid Axios's lossy UTF-8 decode
and BOM stripping. Recording validates UTF-8 fatally and preserves a BOM; the
original caller gets its normal text/BOM or Buffer representation before its
unchanged response transforms, including error-status handling. These are
HTTP-adapter payload bytes after its normal decompression, not HTTP framing or
compressed network bytes. Mutable socket bytes and header arrays are snapshotted
at observation. Async incoming Blob conversion stays ordered and bounded;
outgoing Blobs are refused because public/auth inspection must precede send.
Supported string/buffer sends must be Binance public read methods or ping/pong.

Existing-socket cleanup unsubscribe sends are captured before closure; new
HTTP/socket dispatch stops. Module loading and readiness share a deadline and
race capture failure. Sink failure during unresolved loading, missing books,
unsubscribe failure and incomplete cleanup refuse publication and restore hooks.
Thirty-two focused controls, all 1,114 serial harness tests, 44 root typecheck
and 28 lint tasks pass. Three negative controls fail on omitted outgoing sends,
unsafe large-ID JSON reserialization and omitted typed-auth checking; the
restored implementation passes. Final independent code/docs review is clean.
Relative event times and startup deadlines use monotonic time, with a separate
wall-clock metadata epoch. Actual docs-only landing `e42a32a8e94` is integrated
with all incoming/owned bytes preserved. Full PR-head CI and a fresh separate
guard remain merge gates; no live recording was run.

Example command (requires separately approved live recording; NOT run here):

```sh
corepack yarn -s node --import tsx sdk-rs/harness/record-ts.mjs \
  --venue binance --books BTCUSDT,ETHUSDT --seconds 60 --out /tmp/binance-ts-public.jsonl
```
