# Store notification reachability

Historical scope: the 58 ported rows at ca621d0a2f. Numeric closure at 434a32064a
adds four type rows only: current 62 = the same 34 callable roots plus 28
types/data, so this reachability result is unchanged. Of those, 34 are callable declarations
and 24 are types/data. The first direct grep answered a narrower question than
notification reachability and must not be used alone as proof of this result.

After reading Bybit's STORE_NOTIFICATION_AUDIT.md, all 34 callable source roots
were resolved through the TypeScript AST and their local identifier callees were
walked recursively. All reachable local functions are already among those 34.
The maintained inventory is `tests/fixtures/store-notification-ported-callable-roots.json`;
run `node scripts/store-notification-census.cjs <inventory>` from the repository
root to reproduce the call census (the script path is crate-relative below).

Every nonlocal/property call in the resulting census was inspected: JSON
parse/stringify, String/Number/parseFloat, Math arithmetic, scalar toFixed,
Array.isArray, array includes/some/push/join, Set.has, and string match/replace/
split/endsWith. These are data/native operations in the reference profile.
There are no BaseWebSocket sends, reconnect/close calls, health publication,
BaseExchange snapshot/start helpers, HttpClient execution or other fixed
core-notification calls reachable from a currently ported callable.

Result: **zero affected ported rows by fixed internal reachability**, not merely
zero direct writes. No socket/lifecycle/store-writing exchange method is claimed
ported. Pure history mapping with a resolver and API factories are divergent;
arbitrary host callback effects are not counted as internal Store operations,
matching Bybit's audit scope. Future store writers must use the routed fallible
notification ABI; the zero must be re-audited when those rows are ported.

Reproduction:

`node sdk-rs/safe-cex-venue-hyperliquid/scripts/store-notification-census.cjs sdk-rs/safe-cex-venue-hyperliquid/tests/fixtures/store-notification-ported-callable-roots.json`

This is an audited source inventory, not a subscriber-runtime parity test or a
new CI gate. The Store owner's actual throwing-subscriber witness remains the
reference for the shared repair.

Exchange-state slice: the added TIF/cancel methods and privacy getters reach only
local collections, shared options reads and the runtime clock; none calls a Store
mutation. The constructor is an incomplete FINDING, not a ported writer. A callee
sweep of every production Store mutation therefore still finds ZERO conversions,
including inferred map_err, if-let-Err/to_string, ?, ok, unwrap_or_default and into.
Production placement helpers now read tickers and positions but never mutate.
The cutover at 9419b340d8a6 exposed 26 forwarding methods in the test-only
ReadTrace observer and two fixture Store::update calls; these are not missed
production writers. Forwarders return StoreMutationError unchanged, subscribe
uses StoreSubscriber, and fixture writes assert success. Future production writers
propagate the shared mutation error without inference or string flattening.

Typed-refusal timing sweep: the existing historical single-row mapper computes
resolver/status/price/timestamp before refusing inherited side/type output; its
source corpus includes constructor-side plus throwing status String coercion and
asserts the specific TypeError name/message. Collapse works on raw Values and has
no typed-output refusal. The multi-row exchange historical hook remains pending;
its eventual refusal must be deferred until source row processing and filtering
finish, preserving later source errors and all resolver effects.

Public subscription slice: sendSubscription and sendUnsubscription currently
use an injected fallible sender. Their SOURCE path reaches sendWithDiagnostics ->
BaseWebSocket.send, which may reconnect/close and publish health. This is why
both method rows retain an incomplete sender/lifecycle FINDING; neither is
ported. Concrete shared-socket wiring must preserve this indirect notification
Result. Current native subscription state itself performs no Store mutation.

Public-topic orchestration calls the same injected sender twice then a required
user-topic callback. Source sendWithDiagnostics reaches BaseWebSocket.send;
notification reachability is the same unresolved concrete binding as the two
subscription methods above. This row is FINDING, with no native Store writer.

User-topic dispatch invokes a required fallible subscription callback. Its source
reaches sendSubscription -> sendWithDiagnostics -> BaseWebSocket.send; concrete
notification-carrying composition remains a named finding on the method. The
current owned-key snapshot operation contains no direct Store mutation.
