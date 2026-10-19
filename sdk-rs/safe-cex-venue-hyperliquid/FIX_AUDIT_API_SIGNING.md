# Signing interceptor callback audit

The maintained `addSigningInterceptor` warns through `emitAndLogError` for an
unknown action. If an error listener throws, Axios rejects that request before
its adapter runs. The constructor-installed native API previously reported the
listener failure to the exception sink and then sent the request anyway.

The request pipeline now requires an explicit fallible signing-emission hook.
The binding supplies the real BaseExchange emitter. An execution-local receipt
retains its original ExchangeError while the existing transport/retry adapter
carries a non-network failure marker; the native completion returns the retained
cause before projecting other transport failures. No global registry, lookup ID,
shared error slot or second infallible request path is retained. The separate
response-interceptor emitter is outside this signing row.

`binding::api_signing_audit` obtains the original API factory from an actual
constructed exchange. It attaches a real BaseExchange listener and uses the
native runtime and recording transport. The source oracle extracts the exact
maintained allowlist, interceptor and BaseExchange `emitAndLogError` method and
runs them with installed Axios and tiny-emitter. Telemetry sinks are inert;
emission and its formatting use the original method body. Unknown string, numeric and array action types each exercise a
returning and throwing listener, with two overlapping requests on each client. Native assertions check emission, rejection,
zero transport calls on failure and original cause identity, including a cause
containing a lone UTF-16 unit and negative zero. The invalid configured wallet
also proves the unknown-action branch runs before wallet construction.

The low-level API's preparation/execution hooks are now explicitly fallible;
existing portable tests supply their own callbacks. Response handling, weight
classification, read-only retry and successful signing remain in the same
pipeline. `scripts/api-signing-audit-controls.py` independently ignores the
callback result and replaces its original cause. Both mutations must compile
and fail the named native assertions before exact-source restoration passes.

This bounded probe does not claim arbitrary JS request objects, getter order,
response-emitter exception parity, Promise microtask ordering or arbitrary
external wallet hooks. Those boundaries retain their existing classifications.
