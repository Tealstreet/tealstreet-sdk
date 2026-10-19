# Shared runtime cutover

Both production family constructors install the ported class bodies on the
shared `Js` graph. The former `realm` directory, separate value/Promise engine,
snapshot Store views and orphaned compatibility helpers are removed together.
`host::Host` supplies venue imports and adapts existing host traits; property
access, descriptors, coercion, construction, iterators, JSON and Promise jobs
delegate to `safe_cex_exchange::js_runtime`.

The private module root lives on `ScopeShared::extra`. Installed functions and
constructors capture the scope weakly, preserving lexical class/module context
when invoked. Socket bridges also hold a weak host context. Active calls acquire
their own strong context; an invocation after owner release fails visibly.
Native resources use canonical `ObjectData` attachments. There is no additional
value carrier, conversion fallback or runtime-selection flag.

## Observable regressions

- The frozen emitter probe retains five source events, arities `[0,1,2,2,6]`,
  native number bits, listener observations and real JS stringify output. The
  original emitted array is mutated after emission; native callbacks must see
  that mutation. A compiled array-copy fault preserves ordinary values while
  failing the reference observation.
- Perpetual and Spot public async methods enter the single shared `start_eager`
  boundary. Their synchronous prefix runs at invocation and their continuation
  survives dropping the returned waiter. Independent lazy-family faults fail
  those constructed-binding observations.
- The actual public/private ping classes call the shared timer operations.
  `stopPingLoop` removes real owned timer work, including before controlled-clock
  activation, where startup uses real scheduling with fixed Date/performance.
  The old binding leaves an UNKNOWN task at `realm/promise.rs:103`; the shared
  binding reports classified timer work before cancellation and no work after.
  The maintained classes run independently against Node's real timeout handles.
- The installed `Promise.race` observes existing iterable values through the
  canonical resolver. The constructed `withStartupTimeout` test preserves the
  original result reference and reaches zero pending/blocking jobs on real and
  controlled clocks after cancellation of the losing timeout. This is promise
  scheduling, not domain decoding.
- Native `post()` follows the Axios promise-tail/interceptor checkpoint before
  adapter dispatch. All four weight-limiter scenarios compare the original
  source: adapter count zero at call return, FIFO reservations at 0/150/200,
  discarded waiter continuation and rejection recovery. Ready Rust leaf futures
  are not globally delayed to manufacture this checkpoint.
- The fallible signing-emission hook from #1243 still returns the original
  listener error before transport. Signature diagnostics, custom order-error
  inheritance and unreadable-response reconciliation retain their source tests.
- The native Store facade returns its existing graph, maps and rows. Key
  enumeration does not project unrelated fields; selected getter failures keep
  their original cause. No legacy TableView remains to classify or execute.
- A constructed lifecycle probe holds a weak scope reference, disposes each
  family and drops its exchange. The old strong socket bridge fails the release
  assertion; the weak bridge releases the scope while preserving live callbacks.

## Validation boundary

The actual host library, both binding families and shared capability targets
exercise the production path. Compiler-selected associated aliases remain tied
to their exact shared `Js`/`Thrown` roots and concrete `Host` implementations;
their native/Wasm owner-drift controls are retained. Fixture migrations observe
canonical descriptors/prototypes and native values before typed/JSON output.

Full index, cancellation, override, formatting, strict native/Wasm lint, wiring
and extraction checks remain release gates. The unchanged strict HL496 RELEASE
tape is an additional readiness gate; constructed tests do not substitute for
its HTTP, frame, clock, parser and raw-task observations.

General `Symbol.hasInstance` dispatch remains the separately approved unreachable
omission: shipped source does not define or read that symbol. The host checks
the RHS requirements and delegates to unchanged shared `ordinary_has_instance`,
including bound functions. Existing typed output and browser-host boundaries
remain separately classified.
