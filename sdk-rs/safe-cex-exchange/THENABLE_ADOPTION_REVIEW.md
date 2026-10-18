# Thenable adoption prerequisite

The canonical resolver from #909 already serves the installed Promise constructor,
Promise.resolve and serial iteration helpers. Direct `await_js`, async-body return
adoption and combinators now reuse it for ordinary thenables.

The production cutover follows the merged reporter-context/Bybit migration #928
and a normal master merge. Every await explicitly receives its owning runtime;
returned futures retain that runtime without borrowing the caller.

The approved cutover uses an explicit owning runtime argument for `await_js` and
one wait boundary after canonical resolution. Async return resolves its existing
driver through `resolve_promise`, including native Promise return adoption; it
must not use the static Promise.resolve identity shortcut for the driver. Scalars
still fulfill immediately at body completion. There is no second resolver,
implicit runtime lookup, overload or fallback.

Installed all/allSettled must consume the shared IteratorRecord lazily: resolve
and register each yielded value before the next step. Their native Vec entrypoints
share that operation. A remaining-count sentinel preserves empty synchronous
completion and prevents completion before iteration finishes. Iterator abrupt
completion preserves the original thrown handle and the existing done/close
rules. No array storage or accessor-installation ABI changes belong here.

This follows the ordering in ECMAScript's
[Promise combinators](https://tc39.es/ecma262/multipage/control-abstraction-objects.html#sec-performpromiseall)
and uses Node's actual engine as the independent behavioral oracle.

`tests/js_thenable_adoption.rs` constructs the production NativeRuntime, installs
real builtins and calls the production Promise/await/chain methods. Its only
custom callbacks are supplied JavaScript behavior: thenables, accessors and
iterators. It contains no test runtime, replacement resolver or executor.

The matrix exercises await, async return, both native and installed all/allSettled entrypoints, then, finally and the
already-correct canonical resolve positive control. Cases include synchronous
then lookup, deferred invocation, correct receiver, nested adoption, noncallable
then, getter/call throws, first-call locking while adoption remains pending and
late resolve/reject/throws. Number bits, NaN, raw UTF16 and error/object identity
are observed directly before JSON comparison. Direct-await results are captured
inside the async body, preventing return adoption from masking a broken await.

Iterator cases cover normal completion and throws from next, done, value and
then getters, including continued iteration after a rejected input. Exact FIFO
traces distinguish scalar, native Promise and thenable await/return without
adding scalar wait ticks. Both the Node oracle and native target execute every
case before comparing a group.

Baseline commands (cwd sdk-rs):

```sh
node safe-cex-exchange/tests/js_thenable_adoption_oracle.cjs
cargo test -p safe-cex-exchange --features native --test js_thenable_adoption --jobs 1
```

The original compiled baseline had nine semantic failing groups and two passing
controls. Its tenth failure was a fixture setup error: Symbol-keyed
Object.defineProperty was unsupported before reaching the combinator. The corrected
fixture uses an actual array iterator with supplied next/return methods in both
Node and Rust. Restoring eager iteration now compiles and fails that named group
on the next/get ordering assertion. Exact restoration passes all twelve groups
across 108 engine cases, including canonical resolve and scalar/native FIFO controls.
The Symbol descriptor support census is routed separately; this fixture does not
claim that capability. Source observations remain unchanged through the cutover. Existing
constructor, serial-helper, iterable, microtask and client regressions remain
required. Any changed cancellation/override producer pin must be reviewed and
refreshed in the implementation PR before READY.

Implementation receipts:

- The twelve scoped core Promise/client/runtime targets pass 149 tests (one
  pre-existing ignored test); `js_unhandled_rejections` passes its two native
  Node/host-discard tests. Existing assertions and corpus floors are retained.
- After the normal master refresh, the thenable target and all-features
  cancellation/override/binding gates pass 52 tests with `--include-ignored`.
  The live override check uses the existing isolated TypeScript installation.
- Cancellation review updates eleven changed producers, removes the deleted
  `adopt` and `yield_current` owners, and retains the existing classifications,
  guard bodies, and enabled drop witnesses. All 225 remaining creators are
  classified without findings. PromiseWait and Driver Drop bodies are unchanged.
- Strict native core lint and strict Wasm core/Polymarket library lint pass.
  The Polymarket 96-case signer corpus now uses its existing constructed exchange
  scheduler instead of expecting a single synchronous poll; every source field,
  own-key-order, numeric and exact wire-payload assertion remains in place.
- Lighter/Phemex unit-test targets compile with the explicit runtime API.
