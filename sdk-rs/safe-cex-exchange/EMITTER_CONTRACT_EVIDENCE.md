# Native emitter and error-code boundary

The maintained `BaseExchange.emitter` uses TinyEmitter without a payload
projection. `tests/core-emitter-source.cjs` executes that initializer and the
maintained `formatErrorForSentry` function. It checks original references,
argument counts, own undefined fields, UTF-16 units, once/reentry/throw order,
and eight native error-code profiles.

`tests/support/emitter_retained.rs` reaches the production facade and typed base.
Its six witnesses inspect native number bits, property presence and reference
identity before any finite JSON observations. The original emitter/code bridge
compiled and failed three of these regressions before the repair. Formatter
outputs retain own undefined keys and source property order; throwing code
getters report their cause without recursive error projection.

The constructed Aster private-frame witness is
`safe-cex-parity/tests/aster_worker_emitter.rs`:
`native_private_fill_keeps_infinite_price_through_event_and_worker_post`.
It feeds actual New/Filled frames, observes Infinity in the fill event, and
checks that the required worker DATA capability receives the same object.
The tooling target has three integration witnesses; the Aster crate retains four
private facade/frame witnesses without a worker dependency.
`safe-cex-parity/tests/aster_worker_emitter_control.py` disconnects the actual
worker fill subscription, requires the named private-frame test to fail, and
restores exact source before all three integration tests pass. The ordinary
integration target runs in CI Rust / test, step `Run ${{ matrix.check }}`.

`safe-cex-webworker/tests/worker-data-host.cjs` executes the compiled Wasm
browser materializer. The native Error identity assertion fails with the prior
plain-object materializer and passes after the repair. The actual structured
clone retains Error identity and stack while applying the engine's ordinary
Error-name and custom-property behavior.
The standard TypeError witness also fails the former generic Error/enumerable-only
materializer. It passes with the intrinsic prototype, hidden message and original
stack retained, including a lone UTF-16 surrogate and actual structured clone.

Typed producers call the existing native codec before any JSON serialization.
Previously JSON-shaped fixture observations remain explicit finite projections;
they do not establish an arbitrary-value ingress or host capability.
The owned host codec retains its data-tree profile: shared local object identity
is not a cross-thread capability, and cycles, excess depth and executable values
retain their visible refusals. Local emitter and browser-post identities are
measured separately.
Runtime exception-report callbacks and main-thread worker RPC arguments/success
results retain their separately scoped JSON ABIs. Raw error-input widening is a
separate follow-up, and Candle widening remains held.

The selected native/default contract runner is
`scripts/emitter-accepted-probes.mjs`; `--registry` selects the registry host-error
owner and `NativeAccountFuture` alias. Both runs use fresh compiler discovery and
the ordinary raw conversion trait probes, rather than a replacement contract gate.
The future's expanded error type is the canonical `OwnedJsValue`; the public
registry `NativeValue` re-export remains the same type.

`native_account.rs::emitter_error_tests` checks absent versus own Undefined,
null, exact nonfinite/signed-zero bits, UTF-16 and nested own Undefined in the
actual outbound error envelope. It also polls the actual account future and
observes a retained NaN payload. Fresh registry compiler inventory no longer
selects `exchange_error` or `error_text`: both consume an already typed native
value and return typed native data/text, with no raw schema decoder entrypoint.
Their error-envelope behavior remains covered by these direct production tests.
The selected registry run passes two explicit compiler/trait classifications;
the separate two native unit tests supply the error-envelope and future runtime
observations. These are separate receipts, not a full transitive gate.

`scripts/emitter-error-controls.py` executes both registry error-envelope and
future witnesses, replaces only the production native code capture with a JSON
projection, and requires the compiled own-Undefined assertion to fail. Exact
source restoration must pass both witnesses again. This control executed against
the retained account boundary; it does not stand in for a full-package gate.


Raw error reporting carries `ExchangeErrorInput::Raw(Js)` directly through the
production facade. Seven native regressions cover original Error/message identity,
unread code getters, UTF-16, console-before-coercion, primitive formatting and own
undefined metadata and actual-source accessor read order. The first two and the
accessor-order regression compile and fail before their repairs.
`tests/raw-error-source.cjs` executes thirty-four worker/main profiles from the maintained
reporter and formatter. `core_error_reporting` retains its four typed/source checks;
its finite observer mirrors the oracle's explicit Error name/message replacer.

The reporter's optional message remains UTF-8 and extra data remains JSON. Main-thread
exception hosts retain their explicit terminal JSON ABI; object console diagnostics
use a non-coercing type label at the existing string-only console capability. Worker
posts and local emitter listeners receive original native values. Hyperliquid's
separate realm still uses its existing owned typed-snapshot bridge; it does not gain
shared-realm accessor or object-reference identity from this API migration.
