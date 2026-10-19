# Alternate owner contract cohort

The assigned 33 owners comprise eleven ordinary Deserialize carriers, fourteen
owners with raw-input methods and eight typed constructor/conversion APIs.
The compiler inventory discovers 76 entrypoints across these owners. This batch
excludes the additional lexical `Wire` owners, other Aster cohorts, feature
configurations, aliases and WASM execution.

The eight declared market/position/order/ticker/kline/envelope/placement DTOs
have 435 independently authored field and shape cases checked against maintained
TypeScript declarations. Native projections read every compiler field, nested
placement option and tuple slot, distinguish optional omission from null, and
compare binary64 bits before JSON. Owned, borrowed, text and qualified Deserialize
routes and actual Clone must agree. Positional arrays are refused by strict map
DTOs; the kline tuple requires its twelve declared slots. Raw account/stream
responses continue through their existing production methods, independently of
these declared DTO contracts.

Newer master independently classified the same eleven decoders and repaired
three workflow ingress paths (70102111f5). Its primary contracts and complete
native corpora are retained unchanged. This cohort's additional cases execute
through the existing Deserialize `observation` hook, preserving exact compiler
entrypoints rather than inventing a method for a second corpus. ExecutionOrder
now agrees through owned, borrowed, text and qualified text decoders after the
outer numeric capture repair. CheckParams and ApproveParams now refuse complete
positional arrays through their object-only decoders, while preserving the
individually reviewed nullable tooling fields. Those three obsolete assertions
were migrated with overseer approval; every other field/case is retained.
A prior caller audit found only production native to_typed placement reads;
that historical unused text refusal is superseded by the master repair.

Twenty-two additional source cases execute the maintained public/private socket
classes, real TypeScript DefaultStore and native Map operations against the same
fixed expectations. They cover ticker/mark/trade/depth/kline mutations, private
order/balance/account updates and insertion/deletion ordering.

The fourteen raw owners execute actual SDK methods through the constructed
native Aster binding. Shared fixture code supplies only scripted HTTP/socket
primitives, a deterministic scheduler and a real Store. HTTP config, callbacks,
Map/Error state and Store fields are read directly. The private scope accessor is
injected only beside its owner in the accepted-set sandbox; production APIs and
method bodies remain unchanged. It lets all probes inspect the same binding
scope that the native constructor assembled.

Eight APIs have no raw decoder input: ClockOperation's Future poll, the virtual
clock/realm, private repair and listen-key constructors, prepared-order From
conversions, OrderExecution construction and the generic assembly constructor.
Their exact compiler APIs receive typed classifications. The normal CI
`runProbes` path must emit eight `classificationOnly: true` receipts with no
unclassified panic, separately from the 25 native decoder/runtime receipts.
Constructed witnesses are the real exchange binding target, clock lifecycle,
private repair, listen-key and runtime assembly targets. The new preparation
witness checks every shared/venue option through all three native conversion
routes, including reduceOnly absence/false/true and negative zero.

The selected `scripts/accepted-alternate-contracts.mjs` runner keeps compiler
inventory and exact API validation. Its fixed missing-contract control removes
only these 33 contracts from the in-memory contract map: all 33 actual compiled
owners must then produce an unclassified receipt. That control does not weaken
production discovery or classify neighboring owners.

Verification so far: all 435 declared DTO source cases and 22 maintained
socket/Store/Map cases pass. CI's native probe engine reports all 33 owners,
25 native decoder/runtime observations and exactly eight typed classifications,
with zero failures. The fixed missing-contract control compiles all 33 owners
and reports all 33 missing classifications. Five scoped constructed targets pass
41 tests, including the new complete prepared-option conversion witness.

`scripts/accepted-alternate-controls.py` runs three temporary compiled production
faults: a declared market text field, public trade last-price patch and prepared
postOnly conversion. It requires named behavioral failures (compiler errors do
not count), restores exact source bytes and replays native/conversion gates.
All three controls compile and fail their named assertions: nine native market
field mismatches, two trade-patch mismatches and the typed preparation test.
Exact source restoration passes all 33 owner receipts and the preparation test.
The initial numeric-field control was rejected earlier by the existing numeric
ingress guard; the final declared-text control exercises the output observer
without depending on that guard. Final control receipts are in
`/tmp/aster-alternate-controls.log` (the runner prints its detailed log directory).
No production mutation remains in this change.

The merged-master constructor replay passes all 42 tests across exchange_trait,
live_clock_lifecycle, live_private_repairs, live_listen_key and runtime_assembly
(`/tmp/aster-alternate-constructed-merged.log`). The cancellation policy Node
suite passes 11 tests. The required all-features
`cargo test --locked -p safe-cex-exchange --all-features --test cancellable_future_inventory --jobs 1`
passes six tests with the one explicit discovery export ignored
(`/tmp/aster-alternate-cancellation.log`). Scoped rustfmt, script syntax and
`git diff --check` pass. The first CI parity failure was the independently owned
MEXC reporting observer pair fixed by #873; the overseer accepted that exception.
This record makes no claim that the complete workspace or WASM cohort passed.

The later master merge retains all eleven primary decoder records and all eleven
supplemental corpora. The local marker-bearing merge commit was corrected in a
new commit before push; no history was rewritten. Refreshed primary source738/tooling150 and supplemental source435/runtime22
all pass. All three compiled production controls fail again and exact restoration
passes every33-owner receipt plus the conversion test
(`/tmp/aster-alternate-controls-refreshed.log`). The refreshed real exchange target passes26tests; the cancellation Node suite
passes11 and the all-features inventory passes6 with1explicit discovery export
ignored (`/tmp/aster-alternate-exchange-refreshed.log` and
`/tmp/aster-alternate-cancellation-refreshed.log`).

## Parent retained diagnostic contract

Parent's WsParent `emit_and_log_error` already accepts and directly forwards
`Option<&RetainedJsonInput>`. The row-2 contract still named the former Value
carrier. Its compiler API expectation now records the actual retained type;
production method bodies, the two distinct listener failures and their expected
name/message are unchanged. The included fixture's balance observer unwraps the
Store Result before inspecting fields, matching the current typed read API.
The constructor cohort's two options observers likewise unwrap that Result; their
original boolean expectations remain unchanged.
The canonical scoped replay selects Parent plus the real constructor owner,
which supplies its existing sandbox-only scope accessor. Parent stream-carrier
entries remain separately owned; no production accessor is introduced. The actual
carrier migration is commit 8c148df7ac (retained native reporting contexts), whose
Parent body only changed the parameter type and still forwards the same value.

Fresh master rustdoc plus maintained inventory/contract validation reproduces
`unclassified or changed decoder/conversion entrypoint` with the former row-2
API, then accepts all eight unchanged Parent entrypoints with the retained type.
Logs: `/tmp/kxakrb-aster-parent-api-{red,green}.log`. This is a fixture refusal,
not a production behavioral RED. The canonical native replay remains the runtime
receipt for the actual constructed Parent and listener-error observations.
