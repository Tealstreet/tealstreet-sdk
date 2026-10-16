# Crypto.com socket bodies

Historical slice sheet: the provider-bridge gap and class FINDING statuses described below belong to the original socket slice. The concrete owning-class bindings and their closure evidence are recorded in [GATE_EVIDENCE.md](GATE_EVIDENCE.md), including the full23 runtime-control ledger at39482ee6ce. The original scope, parent and measurements below are retained as historical evidence.

This slice owns only cryptocom.ws-public.ts and cryptocom.ws-private.ts rows.
Its fresh parent is 28bdf6ff0e3a16f050b20c449cca4128d55e7ac3 (merged integration,
including PR7786); there is no unreviewed feature ancestor above that parent.
Exchange/API source and manifests belong to the other lane.

## Scope and contracts

socket_handlers.rs contains the complete message, throttle-initializer, market,
private-account, listener/parser/retry/cleanup bodies. It uses opaque native V
handles, rather than a JSON projection. SocketOps is a mandatory language/host
capability carrier, not an optional fallback implementation. get/index perform
live property access, including getters/proxies; index performs computed access
and ToPropertyKey. key captures the computed assignment key before the RHS.
set/set_index are ordinary Set (including __proto__ setters), delete is the
source Delete operation, and object/array create fresh ordinary literal objects.
spread uses the original iterable and spread_object copies all enumerable own
string AND Symbol keys in source order; object_keys defines the resulting own
properties, rather than applying __proto__ assignment semantics.

invoke receives the already captured callable and original receiver. The body
captures every method/import before evaluating its arguments. truthy/nullish,
equal, is_number, has, numeric, text and tuple_two are language operations;
text preserves UTF-16, numeric retains all JS Numbers, has implements the in
operator including inherited properties/errors, and tuple_two implements the
first two iterator destructuring bindings with source IteratorClose rules.
function creates a fresh retained arrow callback with the supplied name/length
and source lexical receiver. Imported jsonParse, throttle, interval mapping,
book helpers and position-id helpers remain real host dependencies, not rewritten
wire schemas. Math, parseFloat, virtualClock, JSON and setTimeout retain their
live original callee/receiver contracts. The throttle callback, wait and fresh
leading/trailing options are native outputs of the ported initializer; scheduling
is the captured lodash dependency's responsibility.

position_update initiates getPositionMode and catches synchronous initiation
errors before returning its future; await_value adopts the original host promise
and retains its rejection. Resuming the future reads the live original data
array, not a pre-await snapshot. The class promise binding must eagerly drive an
async function until suspension and schedule subsequent completion/rejection;
it must not turn a rejection into a synchronous throw or cancel it when ignored.
The test adapter is finite and only models the selected ready/rejected scenarios.

socket_classes.rs contains real class factories, authored super(parent)
constructors and ordered own-field initialization: 31 public and 25 private
fields. derive resolves class heritage/shape with constructor name/length1;
superclass resolves the ACTIVE constructor's live [[Prototype]], super_construct
retains the distinct original newTarget and returns the original super receiver,
and define creates ordinary writable/enumerable/configurable own class fields.
The new handler/listener factories call their actual native bodies. However,
existing_member is still a provider bridge for previously mapped typed lifecycle,
subscription, valuation and switch bodies. Canonical target names are recorded
but those names do not prove binding/invocation. The two owning class rows are
therefore divergent/FINDING; authored constructor rows are ported at their
explicit super/field protocol. This is no assembled venue-runtime claim.

## Witness limits

The focused socket_handlers target runs 60 independently specified cases against
unchanged maintained TypeScript initializers and full classes. Native decoded
variants, UTF-16 fields, number bits/NaN, original argument/ticker/update/book/parser
identities, errors, callback results and effect order are asserted directly.
Each named group first runs its native cases against literal effect counts, primary
fields, outcome/auth expectations and original identity assertions, then compares
decoded maintained-source observations. This ordering is per group; there is no
wire comparison or claim of global ordering across independent test groups.
There is no decode/re-encode comparison. Heartbeat stringify is a dependency spy:
it captures the native frame before returning an opaque token; no wire codec
or network comparison is claimed. The TypeScript ticker witness uses actual
lodash/throttle with controlled Date.now and timer delivery for leading/trailing
callbacks, and real maintained sort/total/position-id helpers. The Rust host
adapter models that finite schedule; it is not a new production lodash port.

The floor stops at fixture index59 public-super-throw. It includes route priority,
statistics getter reentry, prefix order channels, ticker leading/trailing and NaN,
book fanout, no-op trade, order fallback/overfill, short/zero positions, post-await
live data, duplicate currencies and Symbol-loaded spread, delayed registration
after cleanup, live OHLCV options, two replacement book snapshots with stable
book identity, symbol template coercion order, teardown failure and subclass
construction. It does not enumerate arbitrary accessors, sparse arrays, exotic
iterators, Promise scheduling or all callback/custom builtin behaviors. The host
adapter is a test instrument, not proof that every concrete runtime adapter exists.

Read-only sub-agent review found and the implementation corrected four ordering/
representation mistakes before the measured source commit: original parent-get
failure propagation, public statistics callee capture, channel-before-handler
symbol coercion, and Symbol keys in loaded spread. Review also removed literal class-prototype flags that could not prove native
prototype links, and added direct original-super-receiver and loaded-Symbol
identity checks plus the independent native layer. Source heritage is a separate
assertion; native prototype graph/sharing is not proved. No review cargo gates ran.

## Measurements

Source SHA, actual exits, compiling mutation failures/restorations, manifest
movement and integration preservation are recorded below after measurement.
No workspace compilation, full venue/monorepo suites, host or wasm Clippy, CI,
live exchange sockets, signing, wire codecs or concrete provider assembly ran.

Pre-review-instrument measurements at c25f5baf2d0f602fbe53e4a2ca39515a4b98ea83:
scoped all-targets cargo check exit0, focused socket_handlers target exit0
(12 tests), parity:extract:check exit0 with output Would update0/added0/orphaned0.
These preceded the identity/native-layer instrument corrections; refreshed
measurements below supersede them. Source socket bodies are byte unchanged.

Refreshed source/test SHA: ccf2d25d0f89d28d0c2cf9d81188b17879778cff.
Commands ran sequentially, with one Cargo at a time, from the worktree root and
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript:

- /opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml
  -p safe-cex-venue-cryptocom --all-targets --jobs 2: exit0.
- /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml
  -p safe-cex-venue-cryptocom --test socket_handlers --jobs 2 -- --test-threads=1:
  exit0, twelve named tests covering the 60-case floor.
- node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check:
  exit0; OUTPUT verified Would update0 manifests, added0, orphaned0; census7341
  symbols/512 files. This is the TS extractor, not a Rust parity-indexer claim.
- The required parity:extract writer ran after the official manifest serializer
  changed only the two socket manifests, before the source commit; exit0 and
  OUTPUT Updated0 manifests, added0, orphaned0. No rs array was hand-spelled:
  mappings came from actual local function dependency closures.

Ten compiling production patches were applied TOGETHER on the refreshed SHA.
One candidate Cargo invocation exited101 with twelve named runtime tests failing
and zero passing; it was not ten independently measured Cargo exits. Distinct
named assertions below attribute each family to its corresponding patch. The
whole restored invocation exited0 with twelve passing. Both source files were
restored to exact saved bytes in finally, and scoped git diff --exit-code against
the refreshed SHA for src/tests exited0 afterward. There were zero survivors in
this attempted ten-patch floor; no claim extends to mutations not attempted.

| Patch family / production call site | Observed named assertion |
| --- | --- |
| route_message heartbeat method changed | CRYPTOCOM-SOCKET-INDEPENDENT public-heartbeat / private-heartbeat |
| ticker_update last reads c instead of a | CRYPTOCOM-SOCKET-INDEPENDENT ticker-basic numeric field last |
| throttled book wait100 changed to101 | CRYPTOCOM-SOCKET-NATIVE book-basic number |
| trade_update returns true instead of void | CRYPTOCOM-SOCKET-INDEPENDENT trade-noop void return |
| order_update ref_price replaced by limit_price | CRYPTOCOM-SOCKET-INDEPENDENT order-basic numeric field price |
| position_update contracts uses abs(cost) | CRYPTOCOM-SOCKET-INDEPENDENT position-short numeric field contracts |
| balance_update free uses initial margin | CRYPTOCOM-SOCKET-INDEPENDENT balance-basic numeric field free |
| candle_parser timestamp reads o | CRYPTOCOM-SOCKET-INDEPENDENT candle-basic native numeric timestamp |
| book_parser price/quantity bindings exchanged | CRYPTOCOM-SOCKET-INDEPENDENT book-snapshot-replacement numeric field price |
| public/private_constructor forwards ignored argument1 | NATIVE-PARENT, both public_class_constructor/private_class_constructor |

Tomllib over all six venue manifests at fresh parent versus the refreshed slice
measured 92ported/18divergent/27pending ->104ported/20divergent/13pending:
12 pending->ported, 2 pending->divergent/FINDING, 137 rows unchanged. Iteration
stopped at cryptocom.ws-public.ts.toml, the sixth sorted manifest. Independently
specified socket scope is 8 public+6 private rows; exactly those14 changed.
Socket manifests now total49ported/2divergent/0pending, across51 rows. The 13
remaining venue pending rows in this pre-integration census are exchange/API
work owned by the other lane. Both unowned source files and manifests had scoped
byte-preservation git diff exits0 against the fresh parent.

The sub-agent reviewed production and test instruments read-only. It confirmed
all targeted corrections and the exact14-row mapping partition, and found no
remaining blocker within the stated fixture scope. It ran no gates. No new
shared safe-cex-types field, workspace trait or Cargo dependency was changed.
The explicit unrun list above still applies; in particular no native prototype
links/sharing, concrete older-member provider binding or end-to-end socket
runtime is proved by the class layout tests.

Integration preservation at acc518fa701a7d758622f1b6581cc9221893def7:
fetched/merged faee40c0372da23d6558ad3da511c44a22b05331 from the integration
branch. The only conflict was appended exemption records. Both sides were kept;
tomllib verified the exact union:1386 ours/1422 incoming ->1427 unique records,
with identical shared records and none lost. No socket/source conflict existed.

Scoped git diff --exit-code ccf2d25d0f89d28d0c2cf9d81188b17879778cff HEAD
for the complete crate src/tests exited0. The venue manifests, Cargo.toml/lock,
all five direct safe-cex dependencies, maintained Crypto.com TS directory,
model/book/position-id sources and yarn.lock also exited0. Extractor code and
root package.json exited0 separately. Thus check/test/control inputs are byte
unchanged, and their refreshed-source exits carry forward; Cargo was not rerun
at this merge SHA. Incoming other-venue manifests changed, so the extractor WAS
rerun at acc518fa701a7d758622f1b6581cc9221893def7: exit0, OUTPUT Would update0
manifests/added0/orphaned0, census7341 symbols across512 source files. git diff
--check exited0. merge-base --all with the fetched integration returned exactly
faee40c0372da23d6558ad3da511c44a22b05331, one base; PR diff contains only the
ten socket-slice files. No old integration merge-down payload is being delivered.

Immediately-before-push refresh merged integration
65cdf874589742c3ce300e77c5081a1f9242c6b5 into this branch, producing
343e852c3a3e55f1e088400e03688b82dfa33a9e with no further conflicts. New incoming
changes are the OKX slice; this lane authored no OKX changes. Measured scoped
preservation exits at343e852: crate src/tests0; Cargo/direct dependencies plus
safe-cex-transport and maintained TS inputs0; six venue manifests0; extractor
code/root package0. Source baseline for every comparison is the full refreshed
ccf2d25d0f89d28d0c2cf9d81188b17879778cff SHA. Native check/test/control exits
therefore carry forward without rerunning Cargo at the merge head. Other-venue
manifest inputs changed again, so parity:extract:check ran fresh at343e852:
exit0, verified OUTPUT Would update0/added0/orphaned0, census7341/512.

Tomllib remeasured all six venue manifests at343e852:104ported/20divergent/
13pending, stopping at cryptocom.ws-public.ts.toml. Exactly12 pending->ported
and2->divergent from the original parent, all14 in the owned socket rows.
merge-base --all with fetched integration65cdf874 returned that one base.
The following receipt-only commit changes no measured source/test/manifest/
extractor input; source preservation is verified again before the authorized push.
