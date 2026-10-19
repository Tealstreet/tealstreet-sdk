# Native HTTP telemetry

The maintained `safeCexAxiosCreate.ts` request interceptor at lines 319–358 and
response interceptor at lines 361–419 define the event gate, message construction
and accounting order. Both native factory paths now use `http_telemetry.rs` for
that gate and renderer. The native emitter receives the original UTF-16 message.

Development bypasses endpoint reads. Production examines the owner's current
`grafanaEndpointsEvery` and `grafanaEndpointsPeriodic` fields, in that order.
Periodic request and response timestamps are independent, shared across clients,
and keyed by the source endpoint with the source 30-second/truthiness rule.
Getter/coercion failures before the source try block still reject; rendering and
emitter failures are reported and caught. Canonical Js iteration, property access,
addition and JSON serialization provide those operations.

JS-composed owners register a weak reader of their actual receiver. Realm-backed
Hyperliquid owners read their current realm instance. Typed constructors bind
their existing immutable source endpoint lists through the same capability.
Neither adapter owns another exchange graph or retains an exchange cycle.

The typed NativeHttpHost publishes request outcomes to its bound exchange Store
when no explicit Store was supplied. Existing explicit Store accounting wins,
so a request is counted once. Signing-capture requests retain the existing skip.
Response telemetry follows accounting; request telemetry precedes authorization,
request statistics and terminal dispatch, following Axios interceptor order.

## Independent proofs

The real WOO registry binding failed on zero events before this repair and now
observes the request/response pair. The real Backpack hosted binding observes
the live Store before and after two requests, including development versus
production-periodic event counts. A separately compiled removal of host outcome
publication fails its counter assertion; exact restoration passes.

`js_http_factory_source.cjs telemetry` executes the maintained TS factory with
installed Axios for sixteen development, endpoint and routed/direct profiles.
`js_runtime::http_telemetry_matches_maintained_factory_gates_payloads_and_outcomes`
compares native event text, four clock points and outcome observations with those
source results. The message corpus additionally checks lone-surrogate URLs,
nonfinite body values, own undefined and signed-zero JSON spelling before any
UTF-8 projection. Compiler-authenticated native contracts exercise the reached
gate, renderers, actual Base endpoint capabilities and NativeHttpHost receiver.

Recorded Phemex, Hyperliquid, WOO and Backpack tapes are replayed offline through
the release addon. Reports and captured bytes remain outside Git; independent
business-output or ordering residuals are not normalized to claim telemetry parity.
The current Phemex baseline already matches its 90-row requestStats sequence;
its outstanding difference is the missing request/response telemetry pair.
