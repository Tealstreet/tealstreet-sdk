# Exchange core gate evidence

This records executed controls, not hypothetical checks. Final verification and
review results are appended after the dependency integrations and review fixes.
The detailed bounded suites are recorded in [health and HTTP evidence](HEALTH_HTTP_EVIDENCE.md),
[order evidence](ORDER_EVIDENCE.md), and [client-ID pattern evidence](CLIENT_ID_PATTERN_EVIDENCE.md).

## Executed coordinator controls

| Gate | Deliberate break | Observed rejection |
| --- | --- | --- |
| Package formatting | Insert noncanonical spaces in `BaseExchange::name` | `cargo fmt -p safe-cex-exchange --check` exited 1 and printed that signature |
| Strict clippy | Add identity `map` inside `BaseExchange::name` | Plain clippy exited 101 naming `map_identity`, using workspace warning denial |
| Post-await disposal | Delete the successful private-snapshot response guard | `private_snapshot_does_not_commit_a_response_after_disposal` failed |
| Empty symbol scope | Treat `Some([])` as absent | `scoped_empty_symbols_are_not_a_full_order_reconciliation` failed |
| Signer disposal | Bypass `dispose_signer_waiters` | `disposal_settles_signer_wait_false_and_unsubscribes_without_connected_event` failed |
| Worker request correlation | Accept every string request ID | `worker_correlation_ignores_wrong_request_and_dispatches_override_without_socket` failed |
| Worker timeout cleanup | Omit reply listener removal | `worker_timeout_removes_listener_and_releases_pending_waker` failed before the subsequent Drop-guard strengthening |
| Original validated-payload privacy doctest | Make the private order field public | The exact compile-fail doctest failed because its construction compiled; no diagnostic-code annotation was used |
| Manifest coverage | Add exported `p4aCoverageNegativeSentinel` to the TS base module | Actual `yarn parity:extract --check` exited 1 naming the added symbol |
| Manifest invention | Add public unmapped `p4a_invention_negative_sentinel` | Actual `yarn parity:check` exited 1 naming that item |
| Manifest status | Mark a mapped base row pending while retaining its Rust target | Actual `yarn parity:check` exited 1 naming the contradictory pending row |
| Wasm target | Insert a native-only UnixStream type into socket code | Actual wasm build rejected the unavailable Unix module; the WS control script restored and rebuilt |

All mutations above were restored. The six initial behavioral controls were
followed by a restored 65-test all-feature run, two passing doctests, strict
clippy, and default/all-feature wasm builds. Subsequent review changes require
fresh final results; those earlier counts do not describe the final revision.

The coverage/invention/status controls checked their own named diagnostic, not
merely a nonzero status: the relay dependency row was deliberately pending at
the time, so invention had an independent known failure until that API landed.
No exemption was added to hide that dependency blocker.

## Committed CI controls

`ci.test.mjs` reads the actual workflow and requires a reachable literal test
matrix arm, dependency setup, self-invocation, live generators, all-feature
strict clippy/tests/doctests, `-- --include-ignored`, both wasm builds, broad
source paths and this branch's push trigger. Its mutation cases remove required
commands/flags/triggers and inject unreachable conditions, duplicate run keys,
`continue-on-error`, `set +e`, `exit 0`, `|| true`, and a shell override. Every
case checks the intended rejection. The suite passed 26/26 before the additional
health-policy invocation was wired.

`health-features.test.mjs` reads live Cargo metadata and actual normal dependency
trees. Core defaults exclude regex; the facade necessarily enables health and
includes regex. Removing facade health, enabling core health by default, and
making regex nonoptional each fail their intended assertion. The combined
committed P4a CI tests passed 31/31. The existing workflow/layer audit passed
40/40 after consuming the layer owner's cycle-diagnostic correction.

## Review-driven drop controls

The detached review reproduced a dropped initiating position waiter losing its
store commit, a dropped nuke latching subsequent nukes off, and cross-venue
validated-token transfer. Typed repair and its compiler diagnostics are recorded
in [typed placement evidence](TYPED_PLACEMENT_EVIDENCE.md).

Five additional production mutations each produced the intended test failure
(exit 101), then were restored:

- Remove the detached task's position-store commit: the dropped-initiator test
  sees no positions.
- Remove the pre-spawn guard's host-discard settlement: the unpolled/pending
  task-discard test leaves waiters pending.
- Remove the nuke Drop reset: a subsequent nuke never reaches cancellation.
- Re-arm the nuke cleanup on every call: repeated cleanup/Drop clears the next
  operation's latch, violating the idempotence assertion.
- Retain the worker reply listener on Drop: the cancelled waiter's captured
  waker remains owned and the reference-count assertion fails.

The nuke idempotence test explicitly invokes completion twice, then drops the
old guard while a new operation has latched the same cell. This checks actual
cleanup idempotence without attempting an invalid double destruction of a Rust
value. The host-discard test destroys the task itself, both before its first
poll and while pending; it does not merely drop its waiting caller.

## Pushed contract checkpoint

At `6ca6c73b36`, the combined all-feature/all-target run with `-- --include-ignored`
passed 82 tests, including the live oracles and all three trybuild diagnostics;
both doctests passed. Plain default and all-feature Clippy, formatting, default
and all-feature wasm builds, 31 core CI/feature tests, 40 existing CI audit tests,
and the 37-crate layer graph passed. Coverage extraction changed zero manifests;
invention/status validated 6,901 rows and 934 public Rust items. The informational
report assigned 269 source rows to this core: 267 ported and two divergent.

An independent detached review at that revision passed 26 tests attacking typed
A/B erasure, whole-batch preflight, position task discard/store commit, nuke
recovery and worker cleanup. No further defect was found in those boundaries.
The socket/proxy audit did find additional task retention and malformed decoder
issues; they are repairs after this checkpoint, so these counts are not a claim
about those later changes.

The two declared core divergences are explicit registration of entirely empty
capability overrides (compiled Rust cannot inspect callback source bodies), and
replacement of a lone split UTF-16 surrogate by U+FFFD at the String diagnostic
boundary. Relay constructor divergences remain on the transport owner's rows.
The accessToken async-context provider is intentionally outside the granted
scope; only its shared expiry helper is provided here.

Proxy token follow-up: task disposal and actual JWT codec

- A completion guard exists before the host receives the fetch task. Dropping an unpolled or suspended task settles both waiters with the current cached fallback, clears only its matching flight, and permits a later fetch. Regression checks both poll states with empty and populated cache. Running the same regression against checkpoint 6ca6c73b36 fails with `pending` after task disposal (exit 101); restored implementation passes.
- The 486 expiration-coercion vectors deliberately inject decoded JSON into the actual TypeScript functions. They are separate from 2,817 vectors executing those functions with the actual jsonwebtoken 9.0.3 decoder. The real codec cases include malformed signature alphabet, padding/newline rejection, empty signatures, nonobject/falsy payloads, JSON-inside-string payloads, and lossy UTF-8 decoding. Against checkpoint 6ca6c73b36, the live codec gate fails on array payload at virtual time -1: old Rust expired=true versus TypeScript false (exit 101). Restored implementation passes all vectors using freshly installed isolated dependencies.
- A JWT with exp=100 and signature `bad!` illustrates the production direction: jsonwebtoken rejects it, so TypeScript refreshes; the old Rust decoder accepted it and skipped refresh. The shared helper now applies the actual JWS syntax boundary without claiming signature verification.
- Core-owned CI dependency projection copies repository selectors plus the full frozen lockfile, and installs real TypeScript/jsonwebtoken before the include-ignored core suite in the same change. The 30-case structural audit rejects deletion of the dependency projection, installation or oracle path export; deletion of the real JWT dependency; missing include-ignored; and masked command failures. No other crate's dependency script changed.

## Final merged verification

After `4985ab7ad5` (token repairs), `97fae94700` (socket task ownership), and
merge `0e15953db9` of repaired venue-deps `66316c1e10`, all commands below exited 0.
Live Node tests used the freshly installed isolated TypeScript/jsonwebtoken tree.

| Gate | Final outcome |
| --- | --- |
| Core fmt | Passed |
| Plain core Clippy, default and all features, all targets | Passed using workspace denial; no extra warning flag |
| Default hermetic all-target tests | 73 passed, five Node oracle tests intentionally ignored |
| All-feature all-target tests with `-- --include-ignored` | 88 passed, zero ignored; includes three trybuild diagnostic cases |
| All-feature doctests | Two passed |
| Default and all-feature wasm builds | Both passed |
| Host facade check | Passed with mandatory health enabled |
| Core workflow/feature-policy audit | 35 passed |
| Existing workflow audit after integration merge | 47 passed |
| Layer graph | 37 classified crates passed |
| Health/error generators and client-ID inventory | Freshness checks passed |
| Coverage extraction | 6,901 rows; zero changed manifests, additions or orphans |
| Invention/status | Passed; 941 indexed public Rust items |
| Informational parity report | Core: 267 ported, two divergent, 269 assigned rows |
| Diff whitespace | Passed |

The live suites cover 686 health cases, 18 error-event scenarios, 64 Axios cases,
880 client-ID pattern cases, 130 websocket vectors plus stats/backoff sequences,
486 token coercion cases, and 2,817 real JWT codec cases. Counts describe those
specific axes, not exhaustive behavioral equivalence of the whole core.

Divergence buckets: UTF-16 lone-surrogate replacement is **FORCED** by String;
explicit empty-override registration is **CHOSEN**, with the required future
registry/recorder cross-check documented in CLAUDE. There are no **CORRECTED**
source-bug claims. The accepted gap is that incorrectly declared empty overrides
remain review-only until that venue-driven gate exists; no claim of
self-validating registration is made. The accessToken contextual provider is
outside this grant. No required core gate was skipped; the five default ignored
Node tests all ran in the mandatory all-feature CI-shaped command.

## Shared language shim consolidation

Merged integration 9a45f142cd. Preserved ccxt_precision and other held dependency
helpers, updated its private formatter calls to the public js_number_string,
and deduplicated the trim exemptions. Core JWT coercion, client-ID coercion and
websocket JSON spelling now delegate to that shared function; direct ryu-js
was removed from the core manifest and Cargo regenerated the lock delta.
The whitespace set was already shared: the scanner's single-character wrapper
calls js_trim and contains no table. No shim blocker notes remained to remove.

After consolidation: 93 core all-feature tests including live oracles passed,
9 scoped numeric tests passed (including live 10,034 binary64 cases and exhaustive
Unicode trim), plain all-target core Clippy and all-feature wasm passed. Coverage
reported zero manifest changes; invention/status passed with 946 public items.
The formatter body is unchanged; no whitespace or numeric behavior difference
was observed in these live checks.

## HTTP retry provenance

HttpFailure now carries the original TransportError separately from its message
and optional response. Authorizer errors populate it; HTTP-status failures use
None. Host error enhancement cannot discard the cause. A regression passes
retryable/nonretryable network errors, timeout, host signing failure and HTTP401
through the core, proving missing response is not a retry classifier.
Removing cause preservation compiled and failed the named regression. Restored
core suite: 94 tests passed, zero ignored, including live TypeScript oracles.

An in-memory probe of installed axios and axios-retry (no network) used three
retries and an adapter rejecting with distinct attempt errors. It observed four
interceptor calls with timestamps 1001,1002,1003,1004 and returned attempt-4.
Thus retry exhaustion retains the LAST attempt's full error; a venue retry must
re-enter signing per attempt. This core slice carries provenance only; actual
Binance classified retry composition and its recorder test land in the API slice.

Strict Clippy rejected the inline provenance enum because it enlarged HttpFailure
to 144 bytes. Boxing that optional cause preserves the same error identity and
keeps the Result compact; the 11 focused health/HTTP/token tests, Clippy and wasm
passed after this representation fix. A second axios-retry probe actually HMAC'd
each fresh timestamp in its request interceptor: four signer calls and four
distinct signatures, with the same last-error result. This measures interceptor
re-entry: distinct timestamps prove signing reran, and distinct signatures prove
it did not merely re-stamp a cached signature. Binance production composition
still requires its own recorder test in the API slice.

## Factory params serializer and replacement transport

`remote_serializer_after_tunnel_preserves_hedge_body_omission_and_baked_bytes`
executes a fake remote signer through the real signing adapter AFTER installing
an inner tunnel recorder. It checks signer-before-codec, exactly one callback for
returned params, authoritative omitted body, and byte-identical pre-encoded array
query with no params callback. These are byte-preservation assertions, not claims
of signature validity. The inherited Axios codec-agreement precondition is in
CLAUDE.md. `serializer_separator_fragment_empty_and_refusal` pins question mark
versus ampersand, hash truncation, empty output, and zero sends on codec failure.
A callback returning retryable Network is normalized to Host before HttpFailure
provenance, so it cannot satisfy network_error_on_read's network predicate.

Each temporary mutation compiled, then failed a named runtime assertion:

- Bypass the codec when installing the new transport: remote test failed
  `host cannot reserialize consumed params`; refusal also caught a send.
- Put serialization outside/before signing: remote test failed
  `serialize the signed params exactly once` on unsigned input.
- Restore the original POST body before sending: remote test failed
  `signed body omission must remain authoritative`.
- Re-encode `%20` to `+`: remote test failed `wire query must equal signer encoding`.
  Independently replace the baked URL's `%5B%5D` with `[]`: same assertion failed
  on the alternate-array case specifically.
- Fall back to an empty query on codec error: refusal test failed
  `serializer refusal must produce zero recorder sends`.
- Preserve callback Network provenance instead of normalizing: refusal test
  failed `pre-transmit refusal cannot inherit network retry provenance`.

Restored gates: 96 all-feature/all-target tests passed, 0 ignored with
`cargo test --locked -p safe-cex-exchange --all-features --all-targets -- --include-ignored`;
scoped doctests, strict Clippy and all-feature wasm check passed. Parity check
passed (6,901 rows, 947 public items); extraction check reported 0 manifest changes.
The existing CI core invocation runs these tests; no new optional gate was added.


## Review repairs: reconnect events, insertion order, and managed relays

The reconnect lifecycle test drives actual public/private socket close/open
sequences with queued and inline hosts. Controls that removed emission, armed
only after the inline dial, or emitted before the venue onOpen prefix each
compiled and failed the corresponding reconnect-event assertion. Pending private
authentication does not delay that event. The two JSON field-removal controls
restored Map::remove and failed the surviving-key order assertions independently
(worker telemetry and socket diagnostics).

Relay tests drive 105 automatic closures without entering the native close
counter, fresh headers on each retry, exponential backoff and stable-uptime reset,
the outer 4-second connection deadline, delayed explicit reconnect, force-close
readiness and task cancellation, replacement cleanup, and stale task destruction.
Unpolled, pending and inline-pending tasks are discarded directly; rejected
sleeps release the active run and disposal releases the socket Weak reference.
The persisted descriptor permits a later force-open after task discard.

Temporary controls all compiled and failed behavioral assertions:

- Remove managed classification: `managed relay must not latch core retry`.
- Substitute relay's 15-second deadline for RWS's 4-second deadline:
  `RWS outer deadline must close at four seconds`.
- Make the run guard compare the wrong task identity:
  `discarded relay task must release slot`.
- Remove cancellation/close of the preceding run: the explicit-restart test
  observes its prior transport still open.

These run in the existing mandatory core all-target/include-ignored invocation.
No live venue is contacted; the relay host records dials and feeds protocol frames.

Restored relay slice: 114 all-feature/all-target tests passed, zero failed or
ignored; strict Clippy, all-feature wasm and parity passed. The options-presence
axis remains a declared CHOSEN divergence on createWebsocket, approved for a
separate typed-surface repair; these results do not claim parity on that axis.

## Resync error identity repair

The live WS oracle now drives the real TypeScript `trackConnected` → private
resync rejection for six Error name/message combinations, including the
header-capability error, empty names/messages and BOM/NEL text. Rust drives the
same reopen lifecycle and compares the complete emitted log. Replacing the
projection body with message-only Display compiled and failed
`live_ws_typescript_oracle` at `resync must retain JS Error identity` (missing
`Error:`). Restored: 36 websocket unit tests passed with zero ignored; all-feature
all-target strict Clippy passed. Display itself remains message-only for callers
that consume `error.message` rather than TypeScript `String(error)`.

## Native duplicate-close timer ordering

The live source sequence closes a native socket, receives an in-flight pong that
re-arms ping/pong-miss, then closes it again while reconnecting. Both runtimes
retain the timers: one ping and the 7500ms pong-miss event fire, consuming one
explicit reconnect attempt on the replacement socket. This is a preserved TS
bug, marked at `process_close` and in its manifest row. Hoisting timer clears
back before the already-reconnecting guard compiled and failed
`TS duplicate-close pong miss consumes an explicit reconnect attempt` (0 vs 1).
Restored: 36 websocket tests, zero ignored; strict all-feature/all-target Clippy;
extractor check reported zero changed, added or orphaned rows.

## Reconnect-hook / transport-error channel separation

The live TS timer callback throws `ReconnectHookError: same failure`. Two
sequences cover hook-then-transport and transport-then-hook, preserving one real
transport log and zero suppressed transport duplicates. Rust reports the full
error to the host and retains the source throw's skipped reconnect-latch update.
Adding transport-error handling after host reporting compiled and failed
`hook failure must not produce a transport error log`. Adding it only when a
transport signature already existed compiled and failed
`reconnect hook must not consume transport dedup in either direction`.
The reporting mechanism is explicitly CHOSEN in the processClose manifest row.

The slice review found a hook that starts a dial and then fails could leave its
provisional reconnect listener armed. Registration now commits only after the
hook succeeds; queued and inline Open events from the failed attempt emit no
account reconnect event, while a later successful attempt does. Removing failed
attempt listener revocation compiled and failed `failed hook must revoke its
provisional open listener`. The live TS hook installs a socket before throwing
and confirms zero listener registrations. Restored websocket suite: 37 passed,
zero ignored.

## Dynamic leverage lookup seam

A `dyn Exchange` fixture uses the production DefaultStore: default lookup calls
the venue max-leverage hook and commits both sides, while an override returns
asymmetric leverage and propagates its own refusal without touching that helper.
The override's max-leverage hook panics if the default bypass is taken. Replacing
the trait default with `Ok(None)` compiled and failed
`default leverage hook must return base result`; restored targeted test passes.

## Named socket lookup seam

The test checks all eight source slots with a missing public slot, then replaces
and removes inverse without changing secondary identity. Returning a flattened
list indexed by slot compiled and failed
`absent public slot must not resolve to secondary`. The returned Rc leaves no
borrow guard alive across replacement, and an earlier clone keeps its identity.

## Order and private-snapshot override dispatch

A production Store fixture exercises worker-style dynamic order/snapshot calls
and the actual weak WsExchangeParent resync bridge. Its raw fetch_orders hook
panics if base orchestration bypasses either override. Reverting the bridge to
`venue.base().fetch_and_set_private_snapshot` compiled and failed
`snapshot dispatch must not bypass venue orchestration`. Existing default
snapshot tests retain retry, disposal and scoped-order coverage.

## Complete venue override dispatch contract

The live compiler-API inventory resolves 693 explicit overrides of 41 base
member names across 47 descendant classes. Six missing dynamic hooks now cover
nuke, nukePositions, dispose, setAllLeverage, findOrderByClientOrderId and
refreshMarkets, with base behavior as their defaults. The source inventory is
generated, not transcribed. The existing mandatory core CI include-ignored run
executes its live comparison; ordinary local runs execute the committed-hook and
production-caller checks too.

Controls, each compiled before failing its named assertion:
- Replacing the default nukePositions delegation with success/no action failed
  `generic_nuke_converts_shared_order_into_plain_venue_options` (no Plain order).
- Reverting WsExchangeParent to its base snapshot helper failed the caller gate
  naming `fetch_and_set_private_snapshot` and the bypass.
- Capturing `BaseExchange::dispose` in a function variable failed the caller gate
  naming `audit_captured_bypass` and the captured method.
- Restricting refreshMarkets with `Self: Sized` failed the hook gate naming that
  source override as no longer callable through dyn Exchange.
- Adding a real TypeScript `recordAuthFailure` override failed the live inventory
  naming the missing Rust hook (the source mutation was then removed).

The caller check is deliberately a syntax/alias detector plus a reviewed call-site
inventory, not compiler-level receiver type resolution. Enclosing token-body
fingerprints require review when local receiver or alias syntax changes; ambient
type/field declarations outside the function and unexpanded macros remain a
semantic-review limitation. Same-named methods
on other types are retained in that conservative inventory. Typed placement and
the worker post-hook wrappers have explicit structural dispatch checks.

## Response-bearing REST failures and retry provenance

`RestAdapter` now returns the shared transport `RestFailure`; core's `HttpFailure`
is a transitional alias. A production `HttpClient` used directly as a secondary
adapter retains status, reason, headers, response body, request, message and context.
Replacing its adapter implementation with a response-dropping conversion compiled
and failed `secondary_http_client_retains_structured_venue_rejection_without_conversion`
at `secondary client must retain venue response`.

Network eligibility is derived, never supplied as a bool. The live isolated
axios-retry oracle captures the installed is-retry-allowed deny list (30 codes)
and exercises 76 code/response cases. An independent syn extraction compares the
actual Rust list with the installed list in both directions, so removals cannot
shrink the corpus unnoticed. Removing CERT_HAS_EXPIRED from the installed package
failed the named exact-list assertion; the package was restored. Removing ENOTFOUND compiled and failed its
named case; ignoring response presence compiled and failed ECONNRESET with a
response. Two trybuild stderr fixtures reject bool construction and From<bool>.
Each was independently replaced with valid code, and trybuild failed specifically
because that fixture unexpectedly compiled while its sibling remained green.

A resolved 2xx signature substring is not a rejection and cannot replay a write.
Restoring the old successful-body retry compiled and failed
`resolved_signature_text_never_replays_an_accepted_write`. Rejected signature
responses retain the source retry, including writes and Bybit instruments-info;
the latter's exception applies only to network failures. Exhaustion preserves the
full terminal error. Earlier live axios-retry evidence of four distinct timestamps
AND four distinct fixture signatures remains the re-signing evidence; it proves
new signed bytes per attempt, not merely changing the timestamp field.

Preflight and serializer refusal share non-network failure construction and the
HTTP enhancement path, with a panic-on-send transport. Their counts deliberately
differ: signer/preflight refusal is before common stats (zero); the serializer
runs inside the adapter after stats (one), as measured with real Axios. Inserting
a preflight increment compiled and failed `signer rejects before common stats`.
Both failures retain absent network provenance, even when a malicious serializer
returns a Network variant. CI's isolated dependency projection now includes the
actual axios-retry package under the repository lock; removing that dependency
fails the structural dependency assertion. No workflow step was made optional.

## Fallible keyed websocket dispatch

`MessageHandler` and `dispatch_message` propagate `ExchangeError` through the
existing protocol on-message path. The focused tests preserve captured-list
iteration after map-key removal, missing/empty-key success, stats before the
throwing callback, original error identity and handler release. Replacing
`callback(message)?` with `let _ = callback(message)` compiled and failed
`dispatch_error_stops_captured_list_after_stats_and_releases_handlers` at
`source throw must stop later callbacks`. Restored tests pass.

## Insertion-ordered handler registry

The shared registry uses IndexMap for Binance's non-integer dotted prefix keys.
The test pins insertion order, stable replacement, survivor order after
shift_remove, and tail insertion after deletion. Replacing it with a sorted
BTreeMap (and its corresponding remove API) compiled and failed
`prefix fallback must use registration order, not sorted or hashed order`.
This controls ordering deterministically rather than hoping a randomized HashMap
happens to enumerate differently on a particular run.

## Public socket host type names

`public_host_types` compiles the exact WsRuntime return types using only imports
from safe-cex-exchange. Removing both root re-exports fails that consumer with
unresolved imports naming TransportProvider and TunnelHost. Restored test passes;
no new transport dependency is needed merely to name the core trait signature.

## Omitted versus explicit websocket options

The core takes Option<WebSocketConnectionOptions>: None uses SDK tuning; Some
retains explicit optional RWS tuning. Separate stable host minima preserve the
SDK 1000+random*2000 and library 1000+random*4000 samples. The live oracle executes
actual BaseWebSocket.createWebsocket and the vendored RWS delay function for
omitted, explicit-empty and custom options (three retry depths each). Collapsing
explicit-empty options into SDK tuning compiled and failed the named
omitted/explicit live comparison (1000 versus 3000). Hardcoding the four-second
timeout compiled and failed the explicit-timeout assertion; ignoring max_retries
compiled and failed the zero-retry assertion. Restored socket tests: 40/40,
lifecycle: 13/13, transport relay: 17/17. CI source-trigger audit: 32/32, including
removal controls for both RWS push/PR paths. The omitted-versus-explicit tuning gap is repaired. The row still declares the
remaining representable RWS options (startClosed, maxEnqueuedMessages, debug and
custom constructors) as a FINDING rather than claiming the whole Options surface.
Review also found fractional maxRetries required the previous-counter comparison;
32 live vendored _connect boundary cases pin that behavior.
Restoring the next-counter `>` comparison compiled and failed the named live
previous-counter assertion at a fractional limit; this specifically controls the
review repair rather than relying on the zero-limit test.

## Structured request body boundary (2026-10-12)

`RestBody` explicitly distinguishes untransformed JSON from encoded bytes and has
no Default. Primary and secondary clients retain String/Object identity until a
signing host sees it; final encoding alone makes their wire bytes identical.
Encoded inputs refuse signing without a send. Remote signing retains its separate
source `tryParseJson` step, authoritative signed omissions, and explicit skip path.
This seam test proves representability/forwarding, not Binance signature validity;
the venue's live signing capture remains the source witness for its string-index bug.

Checks: 19 HTTP/token tests, 2 REST gates (including live retry and three trybuild
fixtures), 24 P2c tests, 66 tunnel tests, strict all-target/all-feature Clippy for
exchange and transport, wasm all-feature check, parity (6906 rows/1147 items),
and extractor (zero updates). The merge immediately before this repair retained
55 named workflow steps from both 55-step parents; CI/layer audit 101/101.

Controls restored after execution:
- Encode before HttpHost: compiles, exit 101 at `primary signing must receive
  untransformed data` in the primary/secondary boundary test.
- Replace canonical wire encoder with serde_json: compiles, exit 101 at `request
  transformation must use canonical JS number and own-key semantics` (integer
  key ordering, 10001 spelling, overflow-to-null).
- Make the no-Default fixture construct Encoded explicitly: trybuild exits 101
  because the compile-fail fixture unexpectedly succeeds. The restored fixture
  matches E0277 diagnostic text naming the absent RestBody Default implementation.

No new isolated npm dependency: these added tests are Rust-only, discovered by
native Cargo and the existing mandatory exchange-core gate. The existing live
retry oracle retains its isolated dependency setup.

## Optional message-handler statistics (seam 27)

`MessageHandlerStat.last_callbacks_count` and `had_handlers` retain absence with
`Option`. A source partial `{lastMessageTimestamp: 1}` remains partial through
reconciliation. Recording an existing topic assigns its count but leaves an absent
handler flag absent until handlers exist; creating a new topic assigns both fields.
The live `ws-oracle.mjs` now includes the untouched partial topic alongside the
existing handler-backed and bare-pong cases.

Scoped socket suite: 41 passed, zero ignored, including the live source oracle.
A compiling mutation restoring false-padding on an existing partial topic failed
`partial_message_stats_preserve_absence_until_source_assigns_fields` at
`existing partial topic must not acquire false` (Some(false) versus None).
The mutation was restored before the final passing run.


## Content-type-aware post-signing transform (seam 28)

The installed Axios transformRequest/stringifySafely is executed through a real
request interceptor and recording adapter in `body-transform-oracle.cjs`.
Sixty cases compare both signer input and final wire bytes across missing/plain,
JSON, JSON-with-charset and differently cased content-type values. The corpus
includes raw/empty/JSON-looking strings, JSON whitespace, BOM, NEL, overflow,
malformed JSON, an escaped lone surrogate and 200-level nesting.
JSON parsing precedes trim: valid JSON text is JS-trimmed, invalid text is
JSON.stringify-quoted. Header names are case-insensitive; Axios's value substring
check is case-sensitive. Encoded bodies remain authoritative. The factory and
remote adapter apply this only after signing, using returned signed headers/data.
The existing native core gate installs Axios via its isolated dependency selector;
a selector-removal control requires the package explicitly.

Verified the isolated staged repair with the unfinished signer union removed:
HTTP 20 passed, transport 25 passed, strict Clippy (both crates, all targets and
features) and all-feature wasm check passed. Two compiling mutations failed the
named `wire body must match Axios stringifySafely after signing` assertion:
disabling JSON transformation on raw text, and removing trim on valid padded
JSON. Restored tests passed. The union work was restored after verification.


## Retry predicate and derived timeout budget

`RetryPolicy::execute(adapter, clock)` requires a host clock; `with_delay` accepts
only the delay function. One clock supplies attempt timestamps and sleep. The
engine derives remaining time from the original request timeout, elapsed time
and next delay. Exhaustion returns the terminal failure without another send or
sleep. This is not a caller-carried remaining-timeout value. Source truthiness
retains timeout zero as unlimited and epoch-zero start skipping the subtraction.

`RestFailure.code` retains general host/status codes. The installed library
network-or-idempotent predicate is a separate helper from opaque network-only
classification and from a venue's explicit 503/504 OR. The live oracle executes
real Axios plus axios-retry: 640 code/method/status cases and six deterministic
budget scenarios. A secondary HttpClient recorder preserves ECONNABORTED as well
as structured response metadata. Axios normalizes config methods to lowercase;
the predicate itself retains the installed library's exact method membership.

Controls compiled and failed named assertions when resetting timeout, omitting
retry delay from budget subtraction, or dropping the ECONNABORTED exclusion.
The required-clock trybuild snapshot pins E0061; adding the clock to that fixture
made it compile, and trybuild rejected its unexpected success. All controls were
restored. No signature-validity or retry body-state claim is made by these tests.

Exact staged repair, with pending signer-union changes removed: 20 HTTP tests,
3 retry tests (including live source and trybuild), 25 transport tests, strict
all-target/all-feature Clippy, all-feature wasm and parity passed. Optional
RestFailure context is boxed to offset the general code field's inline size;
its contents and the response/secondary-client preservation assertion are unchanged.

## Owned pre-dial preparation

`start_prepared_websocket` invokes the preparation closure inside the existing
owned task and carries its REST token future through the eventual dial. No venue
handle or protocol disposal hook is introduced. `PreparedWebSocket` supplies the
URL, protocols and optional options; an explicit `None` outcome completes without
a dial. Generations prevent a superseded preparation from dialing or reporting
stale errors. Host discard releases the task slot without a new re-entry latch.

The cancellation witness awaits a real fake `RestAdapter` future and drops the
TASK in unpolled, pending and inline-pending modes, proving both the REST owner
and socket Weak are released with zero dials. Separate tests cover host discard,
a superseded result, preparation errors and inline no-dial completion. Bypassing
the registry compiled and failed `REST preparation must be core-owned`; removing
the post-preparation generation check compiled and failed `only current
preparation may dial`. Both mutations were restored.

With the unfinished signer union removed, 44 socket tests passed with zero
ignored, including the live TypeScript oracle. Strict all-target/all-feature
Clippy, all-feature wasm, parity and extractor check passed. The manifest keeps
the existing unsupported-options FINDING; this operation does not close it.

## One remote signer operation for HTTP and WS authentication

Transport owns `RemoteSignerRequest::{Http, WsAuth}` and the corresponding
response union. Its WS arm uses the exact canonical `safe-cex-types::ws_auth`
payload shared with crypto, retaining extraData.expires, null, overflow lexemes
and deep metadata. There is no transport-to-crypto edge or duplicate payload.
The union arm supplies the operation discriminator; response decoding uses the
originating request because error-only wire replies have no discriminator.

`dyn Exchange::sign_remote_request` dispatches both concrete venue hooks. An
HTTP-only host explicitly refuses WS auth, and an HTTP adapter refuses a WS
response before transmission. Public host signature types are nameable from
exchange core. The HTTP projection remains narrower than arbitrary JavaScript;
the existing canonical WS malformed-input findings remain in the manifest.
Map-only HTTP decoders reject positional-array widening, and explicit null data
or params stays distinct from omission.

The HTTP-only match fixture fails with a matched E0004 diagnostic. Adding its
WS arm made the fixture valid and trybuild rejected the unexpected success.
Bypassing the concrete WS override compiled and failed `remote WS must dispatch
the concrete venue WS hook`. Accepting a WS reply as an HTTP config compiled and
failed `wrong signer variant must never transmit`. All controls were restored.

Scoped checks: 40 core integration tests (HTTP, lifecycle, override coverage,
public host surface and failure/type controls), 29 transport integration tests,
zero ignored, strict all-target/all-feature Clippy, all-feature wasm, parity and
extractor check. The retry body-state finding is separate and remains open.

## Integration merge after signer union

Merged integration `82877523a3` into published core `86aae2c4e1`; the unfinished
retry-body repair was parked on a private throwaway branch and excluded. Seven
conflicts were resolved per hunk. Extractor roots/tests differed in ordering,
not capabilities; both shared JSON parsing and P5 primitive roots remain, with
one canonical WS auth module declaration. The workflow branch list retains the
incoming Ethereum branch and the venue wildcard.

Named CI steps: core 59, integration 70, merged 70, zero missing from either
parent. TOML-parsed exemption union: 266 entries, zero duplicate targets, zero
blank reasons, zero missing from either parent's target set. Cargo's locked
checks pass with num-bigint 0.4.6 (the independent 0.3.3 dependency remains).
The only extractor rewrite was canonical array spacing in Asterdex's incoming
Ethereum mapping: zero added, zero orphaned, final check zero changed.

Checks: 37 extractor tests, 172 CI/layer wiring tests, 99 scoped core tests and
29 transport tests, all with zero failures; required live tests included and
zero ignored. Strict all-target/all-feature Clippy, all-feature wasm and parity
pass. No fleet-wide build is claimed. Changes relative to integration pass
`git diff --check`; incoming dependency-audit text/patch whitespace was retained.


## Retry post-transform data state

The maintained `scripts/retry-body-oracle.cjs` executes installed Axios and
axios-retry, capturing both interceptor data and adapter wire bytes. Four inputs
(object, raw text, empty string, whitespace-wrapped valid JSON), each with and
without a second-attempt interceptor rewrite, produce eight source cases. The
first signed timestamp survives retry. Unchanged retry wire bytes alone would
miss every preimage change, so the test compares both views independently.

`RestBody::Transformed` contains privately constructed post-transform JS data,
not inferred encoded-byte origin. The next signer receives that data and Axios's
retry identity transform prevents a second stringify. Failed configurations
retain signed headers and params before URL serialization, avoiding duplicate
query serialization. Retry classification is REVALIDATED against the original
read classifier. A changed write target returns the terminal error without
replay; the timeout budget remains internally derived.

Five controls were run and restored: removing reclassification compiled and
failed `changed retry classification must refuse write replay`; retaining the
original body compiled and failed `retry signing input must be prior transformed
data`; repeating the transform compiled and failed `retry wire bytes must follow
Axios identity transform`. Making the transformed field public and adding
Default each made its corresponding trybuild fixture unexpectedly compile.
An earlier wire-only mutation did not exercise the transform stage and was
rejected as evidence; the rewrite cases pin the actual identity-transform seam.

Revalidated on integration d8bcccd413 with its canonical lock repair: 29 selected
core tests (including live source and compile-fail cases) and 31 transport tests,
zero failures and zero ignored. Extractor check: zero changed, added or orphaned.
Integration workflow steps increased 70 to 71 without removals. Exemption union
has 277 entries, zero duplicate targets and zero blank reasons. The incoming
truthiness consolidation is retained; how.rs adds only the compiler-required
RestBody arm authorized alongside that delegation.

Strict all-target/all-feature Clippy and all-feature wasm checks pass for the
two changed crates. Formatting check reports only the unchanged incoming import
wrapping in transport lib.rs; no edited Rust file differs.

## Getter override coverage and privacy dispatch

The live TypeScript inventory now includes getters: 697 overrides rather than
693. Newly inventoried are AsterDEX canSign, Hyperliquid and Paradex
isPublicDataOnly, and Polymarket name. The default privacy predicate calls the
venue signing getter; snapshot and account-overlay decisions call the venue
privacy getter. The WS bridge obtains its name through the venue hook.

The runtime witness deliberately inverts key-based classification in both
directions, asserting private request initiation and overlay selection rather
than only getter return values. A base-snapshot-bypass mutation compiled and
failed `private snapshot must dispatch venue privacy getter in both directions`.
Restored tests cover the default signing getter and WS bridge name as well.
A signing-getter bypass compiled and failed `default privacy must dispatch venue
signing getter`; removing getter enumeration failed the live inventory's named
`TypeScript override inventory changed` assertion. All mutations restored.

Wasm-target strict Clippy was run separately from host lint and wasm compilation.
The dependency-inclusive command fails on two pre-existing candle-cache
collapsible_if diagnostics; it is not a full pass. The changed exchange and
transport crates pass wasm all-target/all-feature Clippy with --no-deps.
Restored verification: 25 scoped snapshot/override/socket tests, zero ignored;
host all-target/all-feature strict Clippy; parity and extractor (zero changed,
added or orphaned symbols). Runtime dispatch and syntax coverage are distinct:
the inventory still does not claim compiler receiver resolution or macro coverage.

## Shared account identity and snapshot reads

BaseExchange and a pre-existing WsExchangeParent retain the same options owner.
The public read returns an owned ExchangeOptions snapshot; the narrow patch uses
Unchanged, Set(String), and Remove independently for applicationId/vault. Present
empty strings remain present. This is the existing local, non-Send host contract,
not a new restriction introduced by the owner.

The source-constructor oracle executes the actual constructor body with inert
store/emitter boundaries and proves retained aliases see identity changes. Rust's
retained snapshot deliberately does not: the constructor row is CHOSEN divergent,
while fresh base and pre-existing WS reads both observe the patch.

Two production mutations compiled and failed named assertions: retaining an
options borrow across the pending snapshot request failed `identity patch must
not retain an options borrow across await`; cloning the owner into detached state
failed `pre-existing WS bridge must read patched identity`. Both restored. The
panic catcher exists only in this native regression witness to turn a mutant
RefCell panic into a named failure; production does not use catch_unwind.

Restored scoped snapshot/override/socket tests: 27 passed, zero ignored, including
the live constructor and source override inventory. All test targets compile.

Integration merge for the identity ABI takes 9222ed2454, including the reviewed
stringify delegation and candle-cache wasm lint repair. Exemptions were unioned
as parsed TOML blocks: 280/280 parent entries become 283 unique entries, zero
blank reasons, both parent target sets retained. Named CI steps 71/73 become 73,
zero missing from either parent; 185 CI/layer wiring tests pass. The sole Rust
compatibility edit outside core changes the incoming OKX test host's WsParent
options return to an owned clone, matching the new trait without venue behavior
changes. Dependency-inclusive wasm all-target/all-feature strict Clippy now
passes for exchange and transport (no --no-deps); the prior dependency failure is
superseded on this merged tree. The 27 scoped core tests and extractor check pass.
The canonical alias declaration merge at 935fc8f578 also carries the reviewed
shared random primitive in its ancestry. Its step union is 73/74 -> 74 (including
repeated names, neither parent's multiplicities lost); parsed exemptions are
283/284 -> 287 unique, zero blank reasons. Final CI/layer wiring tests: 190 pass;
extractor still reports zero changed/added/orphaned. BaseExchange.constructor now
cites owned-snapshot-vs-live-alias while retaining its specific two-view witness.
The incoming OKX public_socket test target compiles against the owned options ABI.

## Axios header assembly and generated body metadata

The maintained `header-transform-oracle.cjs` executes installed Axios and captures
both interceptor input and adapter configuration for 72 cases (four methods,
nine data states, absent/explicit JSON content type). It includes the default
Accept header, factory/request case collisions and a signer adding a later case
variant. `axios_header_pipeline_preserves_signer_and_wire_views` compares the
complete captured header maps and signing data. Adapter non-null primitives use
the transport's existing byte projection; this is not a claim that Node and XHR
accept identical primitive network bodies.

Five controls were compiled and failed at the named assertion, then restored:

| Mutation | Assertion |
| --- | --- |
| Case-sensitive merge comparison | `pre-sign header normalization must match Axios` |
| Omit the post-sign normalization pass | `post-sign headers must match Axios` |
| Suppress object/array JSON Content-Type generation | `post-sign headers must match Axios` |
| Replace generated form Content-Type with JSON | `post-sign headers must match Axios` |
| Transmit null as literal bytes | `null and generated-content-type wire body must match Axios` |

The remote-path test also pins normalization on both sides of signing, generated
headers, absent null wire body, and retained explicit null in the failed request
for retries. The prior union test's literal-null-byte expectation was wrong:
explicit null remains observable in the decoded signer envelope, while Axios
without JSON Content-Type sends no body. Plain-object serialize_headers retains
case duplicates; normalization is deliberately not added to that source branch.

Validation: core HTTP/health/token 23 passed with ignored oracles explicitly
included; transport p2c 33 passed; restored header oracle passed; strict host
Clippy and dependency-inclusive wasm32 Clippy each passed for exchange and
transport with all features/all targets. Parity passed (6984 rows, 1284 public
Rust items); extraction reported 0 changed manifests, 0 added, 0 orphaned.
All Cargo commands used jobs=2 and ran serially.

## Callback-owned asynchronous message work

`start_message_operation` owns only a message callback's asynchronous tail.
The protocol test drives real BaseWebSocket::on_message twice: synchronous prefix,
next message while the first tail is pending, then tail completion. A recording
runtime receives the rejection exactly once; the same-text transport error still
logs afterwards, proving the rejection did not consume transport deduplication.

Disposal tests drop the actual unpolled, pending and inline-pending tasks. An auth
cleanup guard is captured before future construction; task drop clears its latch
and releases the socket Weak. No connection generation is reserved and no dial
runs. Controls compiled and failed named assertions: bypassing the registry hit
`message operation must be core-owned`; omitting reporting hit `unawaited message
rejection must be observable at host boundary`; reserving a generation hit
`message work must not reserve a connection generation`.

Host delivery beyond the recording runtime remains an integration obligation.
There are no production ExchangeRuntime implementations in this tree. The trait
now requires observable reporting and the runnable example no longer teaches a
no-op reporter. This is a CHOSEN host projection, not equality with a JavaScript
unhandled rejection, and not evidence that an absent host implementation works.

Consumed integration 91db928805 by fast-forward: named workflow steps 74 to 76,
zero removed; parsed exemptions 301 blocks, 301 distinct, zero blank reasons.
The numeric Store ABI required only test-fixture signature adaptation in core;
all-feature/all-target core cargo check passed after that separate compatibility
commit. No stash, lock regeneration or whole-side conflict resolution was used.

Callback repair validation: 45 websocket tests plus the separately executed live
TypeScript oracle passed (46 total, no oracle left unexecuted); doctests passed;
strict host and dependency-inclusive wasm Clippy passed separately for core,
all features/all targets. Parity passed (6984 rows, 1429 public Rust items), and
extraction reported 0 changed manifests, 0 added and 0 orphaned symbols. The
status gate initially requested exact adapted-target paths in the changed row;
those paths were added and the gate rerun successfully.

## Remote signer baseURL spelling

SigningConfig now explicitly renames base_url to `baseURL` in both its serializer
and strict wire decoder. The full HTTP union serialization test asserts the exact
field name and absence of `baseUrl`, then decodes a source-shaped envelope.
Removing both rename attributes compiled and failed `remote signer must receive
baseURL with exact protocol spelling`. Restored transport p2c: 34 passed, zero
ignored. This small protocol repair is separate from pending remote transform
ordering and the venue interceptor entrypoint.

## Explicit URL-encoded request data

`form_transform::explicit_form_transform_matches_live_axios_signing_and_wire_views`
executes installed Axios through its request interceptor and adapter in 19 cases.
The installed branch is `toURLEncodedForm` / `URLSearchParams`, not the venue's
query `qs` serializer. The shared post-sign transform retains structured input
until signing, then emits form text, including `x=a+b&y=2`. Retry configuration
retains that text and the identity transform does not encode it again.

The corpus includes flat/nested arrays, bracket and JSON metadata tokens,
integer-key ordering, trimmed keys with original recursive paths, sparse
array-like objects, null omission, fallible object coercion and the installed
100-level depth boundary. It covers the JSON request-body surface, not File,
Blob, Date, custom formSerializer options or cyclic JS objects. URLSearchParams
percent encoding is separate from the factory query codec. Source error codes
and JS exception classes remain subject to the existing TransportError host
projection; this test pins refusal and no produced wire request.

Controls replace form output with JSON and replace plus spaces with percent-20;
both compile and fail `form wire encoding must match Axios including plus spaces`.

Merge-fixture caution: the HOW heartbeat test can acquire `leaf_error` at both
future construction and host execution when parents wrapped different sites.
Only one conversion is valid. The compile repair at c44d1062cd removes the
second conversion; its scoped deadline/cancellation test passes. This is a merge
artifact pattern, not evidence that every integration head contains the defect.
Validation: restored 19-case form oracle passes; 24 HTTP/token tests pass;
exchange and transport host strict Clippy and separately wasm strict Clippy
(`--all-features --all-targets -- -D warnings`) pass. Parity validator passes.
The native core job already installs Axios from the SDK selector and runs this
non-ignored integration test in its all-targets command.

## Venue interceptor ordering

`venue_interceptor_runs_after_defaults_without_reapplying_deleted_headers`
compares actual Axios async request interceptors for clear, replace and reject
cases. Factory defaults are visible to the callback; its complete header map,
baseURL and timeout deletions survive to the adapter. Callback refusal uses the
shared error path and counts zero common requests. Reapplying defaults after the
callback compiles and fails `interceptor header deletion must survive without
default reapplication`. Restored HTTP/token suite: 24 passed, zero ignored.
Host and separately wasm strict Clippy pass for exchange and transport; parity
validator and extractor (zero changed/added/orphaned) pass. No public skip flag
or prepared-request bypass was introduced.

## Remote server/client transform staging

`remote_stage_matches_whole_source_adapter_and_real_axios` executes production
remoteSigningAdapter.ts, production tryParseJson and diagnostics, installed Axios
on both server and browser paths, and real nanoid from the SDK's frozen selector.
Twenty-seven cases compare the complete remote config (including baseURL),
browser interceptor data/headers, and final adapter headers/bytes across absent,
JSON and form content types. Primitive JSON stays transformed text; only object
and array JSON is restored before browser signing. Local signing ordering is
unchanged. The byte projection retains the existing null/no-wire-body contract.

Removing adoption of the server-transformed data compiles and fails
`remote signer must receive server-transformed data and generated headers`.
Restored: 27 live cases, 34 transport tests, 24 core HTTP/token tests; CI dependency
wiring, host strict Clippy, separately wasm strict Clippy, parity and extraction
(zero changed/added/orphaned) pass. The oracle is ignored in the generic native
matrix and mandatory in the core include-ignored job, after that job installs
its frozen Axios/TypeScript/nanoid dependencies. A nanoid-removal control fails
the selector assertion. No root runtime module fallback is used by the oracle.

## Shared waiter cancellation

PositionFetch and proxy Flight now share private per-future waker registrations.
Dropping a caller releases its waker before shared settlement without cancelling
shared work. Re-poll replaces only that future's waker; identical executor
wakers do not merge registration ownership. Clone, replacement destruction,
wake and removal destruction run outside RefCell borrows.

Five scoped tests pass. Removing Drop cleanup and clobbering all registrations
compile and fail named release/sibling assertions. The unchanged public-API
finding tests from 02ae9f76d4 were run as temporary controls: BOTH fail at their
explicit `finding flipped: cancelled ... waiter now releases its waker`
assertions. The source finding file was not copied into this commit.

Restored HTTP/token tests: 24 passed. Caller inventory, host strict Clippy,
separate wasm strict Clippy and parity pass. New Waiters source provenance is
mapped to both source owners. Local extraction reports zero changed/added/orphaned;
RESUME.md later established that the extractor's smol-toml dependency is missing
from clean-install wiring, so this local execution is not evidence of clean-tree
reproducibility. That wiring repair remains with the gate owner.

## Inherited handler stats read boundary

The maintained live WS oracle now executes 78 cases against actual
BaseWebSocket.recordMessageHandlerStats/reconcileMessageHandlerStats: all twelve
prototype names and a missing ordinary key, each with absent/empty/two own
handlers and absent/present own stats. It records typeof before serialization,
counts including undefined arity, own-key presence, and the two __proto__ cases
whose global prototype mutation makes reconciliation throw. Canonical TableHit
arity and absence of invented own records match; mutation of the shared
inherited referent remains a FINDING on record/reconcile/cleanup, not closed.

Two restored mutations compiled and failed named live assertions: own-only
handler lookup failed `stats must retain inherited handler arity including
undefined`; manufacturing own stats records failed `inherited stats must not
become own records`. Restored oracle passed. Scoped socket tests: 45 passed,
one live oracle separately executed with --include-ignored. Host all-target
strict Clippy and wasm32 all-target strict Clippy separately passed for
safe-cex-exchange. Root parity passed on this branch (6984 rows / 1430 public
items). No merge-up or clean-install extractor claim: RESUME.md documents the
extractor dependency wiring defect and the unrelated integration registry gate.

Stats error follow-up: record_message_handler_stats is fallible, and dispatch
propagates the measured immediate __proto__ reconciliation TypeError before
callbacks. The global write remains CHOSEN FINDING: a global side table could
represent it, but reproducing process-wide corruption is deliberately declined.
The omission-of-error mutation compiled and failed `source prototype
reconciliation throw must be a caller-visible error`. Restored 47 socket tests
(including the 78-case source oracle) passed. Separate host and wasm all-target
strict Clippy and parity passed. Cross-call effects of the declined global write
remain expressly outside the implemented projection.

## Remote adapter output stage

The whole-production remoteSigningAdapter.ts oracle now runs 54 cases: real
server Axios with real browser Axios, and the same server with an echo signer.
It compares exact inner-adapter data identity (object, array, string, null and
omission), not just projected bytes. An HTTP-client test independently proves
the wrapper retains an echoed object and a deleted Content-Type. Two mutations
compiled and failed named assertions: remote re-encoding failed `remote inner
adapter must retain echoed object versus string identity`; wrapper re-encoding
failed `HTTP inner adapter must preserve remote echoed object identity`.

Restored results: 54 source cases, 34 transport p2c tests, 24 HTTP/health tests
(one unrelated ignored test not included), and 47 socket tests including the
live oracle. Host and wasm all-target strict Clippy separately passed for
exchange and transport; parity passed 6984 rows / 1430 public items on this
branch. Five previous transport assertions embodied the extra output transform;
they now assert the actual source adapter boundary rather than wire projection.
Response-side parser provenance is separate pending work and is not closed here.

## Staged integration merge: 6bcff79504

First merge-down stage retained all 76/78 named CI steps from the two parents,
78 merged and zero missing from either. Real TOML parse: 303 exemption blocks,
303 distinct, zero duplicates, zero blank reasons, zero parent entries missing.
No parent dependency table became empty; lock changes only add GRVT/KuCoin
workspace package dependencies. Exchange all-feature tests: 167 passed, zero
ignored across 17 binaries; transport p2c: 34 passed. Host and wasm all-feature
all-target strict Clippy passed separately, as did parity (6984/1546), layers
(78), routing (47 exchanges/1128 cases), and local extractor drift (0/0/0).
Extractor success uses this machine's pre-existing smol-toml install and does not
prove the clean-install wiring fixed. Final merged-state review is still pending.

## Staged integration merge: 8cbfb8b5f9

Second stage retains all 78/80 named CI steps, 80 merged, none missing from
either parent. Real TOML parse: 303 blocks / 303 distinct exemptions, zero
duplicates, blank reasons or missing parent entries. No nonempty parent
dependency table became empty; lock changes add Binance/Bybit workspace deps.

The inherited override scanner treated standalone #[test] functions as production.
It now excludes that explicit attribute, with no filename exemption; unannotated
helpers remain conservatively audited. A compiling removal control fails
`test-only teardown must be excluded without hiding the production sibling`.
Inventory reconciliation removes a test-only candle-cache start reference and
adds two production Bybit parent.name() reads; no production dispatch edge was
removed. Those reads use the socket parent bridge.

Exchange all-feature tests: 168 passed, zero ignored across 17 binaries; transport
p2c: 34 passed. Host and wasm all-feature all-target strict Clippy passed
separately. Parity: 6984 rows / 1764 public items; layers: 78; routing: 47
exchanges / 1128 cases / 43 routes. Local extractor: zero changed/added/orphaned;
its pre-existing smol-toml install remains a clean-install evidence limitation.
Final merged-state adversarial review remains pending.

## Staged integration merge: b8fc4dfbaa

Third stage merged without textual conflicts: named CI steps 80/82 -> 82,
none missing from either parent. Real TOML: 312 blocks / 312 distinct, zero
duplicates, blank reasons or missing parent entries. No nonempty parent
dependency table emptied; lock changes only add AsterDEX/Lighter dependencies.
Exchange tests: 168 passed, zero ignored in 17 binaries; p2c: 34 passed.
Host and wasm all-feature all-target strict Clippy passed separately. Parity
7334/2170, local extractor zero changed/added/orphaned, 78 layer tests and
47-exchange/1128-case routing pass. The extractor's existing local dependency
caveat still applies. AsterDEX oracle.cjs:57 has inherited trailing whitespace
in the integration parent; this merge does not edit that venue's oracle.
Final merged-state review remains pending.

## Staged integration merge: b283acad08

Fourth stage: CI named steps 82/87 -> 87, no parent losses; TOML 323
blocks / 323 distinct, zero duplicates, blank reasons or parent losses.
All normal, dev, build, workspace and target-specific dependency tables checked
against both parents: none emptied. No lock package identity removed; layer
exceptions and grants retained in their respective containers.

The gate exposed two real MEXC privacy base-helper calls; both now invoke
Exchange::is_public_data_only. MEXC library check passed. Separately, the scanner
now distinguishes direct self fields declared BaseHandler/Rc<BaseHandler> from
exchange bases. It still inventories the worker call; a compiling removal
control fails `worker BaseHandler exclusion must retain both exchange base
bypasses`. This remains syntax-only, not compiler/import-alias resolution.
The inventory adds 23 reviewed MEXC/worker call sites, removes none and changes
no previously inventoried enclosing body fingerprint.

169 core tests, zero ignored; 34 p2c tests; strict host and wasm all-feature
all-target Clippy separately passed. Parity 7334/2529, local extractor 0/0/0,
78 layer tests, 47-exchange/1128-case routing and 32 CI wiring/control tests pass.
Local extractor install caveat remains. Integration advanced during these gates;
the remaining delta and final adversarial review follow before readiness.

## Final reviewed integration merge: 82445b433d

Returned adversarial sub-agent review: no new merge-blocking findings over the
complete integration-to-working-tree diff, including waiter identities, remote
input/output phases, inherited stats and the override audit repairs. Reviewer
ran serial gates: 169 exchange tests (17 binaries, zero ignored), 34 transport
tests, 2 exchange doctests, 32 CI tests, separate strict host and wasm Clippy,
parity 7334/2534, local extractor 0/0/0, 78 layer tests, routing 47/1128/43.
Named controls were inspected from failure logs, not rerun by the reviewer.

CI 87/87 -> 87; TOML 327 blocks / 327 distinct, zero duplicate/blank/lost
entries. Normal/target/dev/build dependency tables and lock package identities
retain both parents; layers and lock match this integration parent exactly.
No clean-install or browser execution claim. Reviewer additionally identified
an unchanged params-serializer truthiness copy treating lexical 1e400 as false
in http.rs; it is a separate pending canonical-truthiness repair.

## Final incremental merge review: 1d4dc0c02e

Returned incremental adversarial review: no blocking findings. All 19 incoming
KuCoin/docs/script files match integration; no core, transport, compiled shared
dependency graph, lock or layer change from the fully tested previous stage.
Prior 169 core / 34 transport / 2 doctest and separate host/wasm lint evidence
therefore remains applicable. KuCoin was reviewed for merge preservation, not
independently re-audited as a venue port.

Affected checks rerun: 5 override tests with zero ignored; parity 7334/2536;
local extractor 0/0/0; layers 78; routing 47/1128/43; root CI wiring 237; core
CI contract 32. CI steps 87/87 -> 87 with no losses; exemptions 327 blocks /
327 distinct, no duplicates, blanks or parent losses. KuCoin oracle targets
16 -> 19 with none removed. Incremental diff --check passed.

### Response adapter provenance (response ABI follow-up)

`RestResponse.body` now requires `RestResponseBody::{Encoded,Untransformed,Transformed}`.
There is no Default and no implicit byte view. Ordinary HOW frames retain native
values, custom-parser output is Transformed, and a subsequent source transform
receives current data even after a previous parser. `json_value` is a separate
model projection; it does not claim an interceptor ran. MEXC's existing response
interceptor uses the current-data operation; the other consumer edits select an
explicit constructor or read the current model. This does not implement the
pending Binance public factory or claim the other pending factories complete.

The original Binance witness from f6de9ca8c4 was run before changing its assertion:
it compiled and failed `FINDING decoded HOW string loses response provenance
"123"`, with Transformed(String("123")) versus Encoded quoted HTTP bytes. The
maintained `public_factory_response_provenance_finding` now compares 22 actual
factory/HOW/Axios captures against the shared next-parser operation, asserting
`typeof` and recursive numeric bits before serialization. Direct String("123")
and HOW Number(123), bits 405ec00000000000, differ; object, array and null controls
agree. The historical test name is retained so existing mandatory venue wiring
continues to run it. Isolated dependencies came from ci-deps.cjs with a frozen
offline install, including Axios, axios-retry and qs.

Three temporary production mutations each compiled and failed a named assertion,
and were restored: replacing HOW Transformed output with serialized Encoded
bytes failed `HOW custom parser must retain transformed provenance`; bypassing
transform for Transformed data failed `response current-data parser typeof HOW`;
re-encoding ordinary HOW data failed `ordinary HOW response must retain native
untransformed data`. Dedicated native-value tests retain insertion order, signed
zero bits, overflow as a number, nested shapes and caller-visible parser errors.
Trybuild pins E0277 for Default and E0308 for byte coercion, with an explicit
three-variant constructor case that compiles and runs.

At this checkpoint the census of Rust RestResponse users is core, transport and
six venues. Binance, KuCoin, MEXC and OKX test targets compile. Bybit and WOO's
committed-base stats Result call sites require their owners' already-pushed
compatibility merges (7180768a0a and 8fe451d3cc); those unrelated propagation
fixes are not duplicated in this response change. Final merged consumer coverage
must consume them before claiming all six compile.

The added whole-HOW native-value test first failed its signed-zero bit assertion
(0 versus 8000000000000000). This exposed an earlier envelope `from_value` buffer,
not a loss inside RestResponseBody. The repair detaches the native payload from
the cloned frame before decoding typed envelope metadata, removing that payload
buffer rather than inventing another numeric parser. The restored test passes
with native ordering, negative-zero bits, overflow-number and nested-shape checks.
A fourth compiling mutation routed only the detached payload through
serde_json::from_value; it failed `ordinary HOW response retains signed zero
bits` (0 versus 8000000000000000) and was restored. This control preserves the
new enum phase, so it independently checks numeric preservation rather than
merely checking which variant was selected.
The envelope audit is bounded: the shared optional RequestDump.body Value still
passes through metadata decoding. It is not the returned response payload and
response diagnostics read the original frame before that decoder. No claim is
made that every diagnostic field's rebuffer has been repaired.

Restored core verification: 170 tests across 17 binaries, zero ignored, plus two
doctests. This explicitly includes the live remote, retry, JWT and TypeScript
oracles using core's isolated dependency selector. An earlier attempt used the
types-job selector and failed loudly on missing jsonwebtoken; it was corrected by
selecting the core install, not by skipping the oracle or changing its imports.

After the envelope finding, the maintained source corpus was expanded from 16 to
22 cases with lexical -0, overflow and negative underflow on both direct and HOW
paths. Numeric tagging uses the canonical lexeme-aware accessor, and all 22
type/bit comparisons pass. Transport verification is 152 tests in nine binaries,
zero ignored, plus six doctests. Both live MEXC client composition/control tests
and both Binance public API witness/import-control tests pass.
Core and transport strict Clippy pass separately on the host and
wasm32-unknown-unknown targets, both with all features and all targets. This is
lint/build coverage, not browser execution. Cargo work was serialized with two
jobs. The response commit intentionally precedes the Bybit/WOO compatibility
merges and the Store notification ABI migration.
Structural checks: parity 7334 rows / 2541 public Rust items; extractor zero
changed, added or orphaned manifests/symbols (local hand-installed smol-toml
caveat remains, not clean-install proof); layer suite 78 passing; routing 47
exchanges / 1128 cases / 43 routes, fresh. Real tomllib parse: 332 exemption
blocks, 332 distinct targets, zero duplicates or blank reasons. No CI workflow,
Cargo manifest or lock changes in this response diff. Adversarial review is
requested before the atomic response commit; integration readiness additionally
requires the compatibility merges and Store-first sequencing.
Adversarial review returned no blocking findings for the response diff against
aa632b5578. The reviewer independently passed three response tests, 68 tunnel
tests and both Binance tests including the 22-case live-source witness; verified
332 parsed/distinct exemptions and diff whitespace; inspected all four compiling
mutation-failure logs without rerunning mutations. Its first Binance run selected
the core dependency set and failed loudly on qs; the types-job set passed. This
disposition covers the atomic response change, not subsequent dependency merges
or Store notification migration.

## Notification cutover candidate (Store-first branch)

This slice consumes published Store notification/cause/constructor commits through
31009e196f, without the separately reviewed response-provenance branch. Listener,
emit/log, cleanup and socket-parent outcomes are fallible; Store's opaque cause
recovers the original ExchangeError. The live oracle runs installed TinyEmitter
and extracted actual findBestPrice/emitComplete/waitForRemoteSigner bodies. Worker
ordering is a consumer contract here, not a claim that worker migration is done.
The new real-Store test uses DefaultStore's production constructor and checks
committed setting, skipped later listener/subscriber, unchanged-setting silence,
and original TypeError metadata. Snapshot tests pin notification retry, recovery
throw before sleep, and origin-only notification failure for shared fetches.

Two compiling controls were run and restored: ignoring callback Result fails
`source throw must stop later emitter listeners`; rebuilding only its message
fails `recover original listener name and metadata`. The latter loses TypeError,
code, config, request, stack and context and is rejected by the test. The source
oracle includes seven fanout/caller-order cases and two signer-status cases.

Inventory review: all 115 creator owners retained, no classification removed;
23 producers changed for fallible returns/callback reporting, catch blocks and
explicit error inference. Three new inner async blocks are catch scopes, not
independent tasks. Existing guards remain pre-spawn and run on error returns;
the signer waiter unsubscribes before its source settled-but-unresolved failure.
Formatting-only brace/comma token changes were inspected after rustfmt. Override
inventory retains every prior call, updates notification-related bodies and adds
three accepted integration calls (two Binance socket disposals and Bybit's
WsParent snapshot dispatch); the two Store update_order forwards still propagate.
No new venue base-helper bypass was accepted.

The candidate 7e424da63d scoped tests and two doctests passed; its two stale inventory failures were repaired and rerun successfully. Together with the added resync-log guard test, that candidate set was 189 core tests plus two doctests, zero ignored. Strict all-feature/all-target host Clippy and wasm Clippy passed separately with one Cargo job. Parity passes 7334 rows / 2801 public items; local extractor is 0 updates / 0 added / 0 orphaned after the standalone parsed-data-identical journal spacing repair c7b4919e6d. Layer tests 78 and core CI tests 32 pass. Current exemptions parse as 359 blocks / 359 distinct / zero blank reasons. Final merged-state review and integration merge remain outstanding; this is not an integration-readiness claim.


### Candidate review corrections

The reviewer rejected 7e424da63d on two measured defects. The new resync test
itself pinned an incorrect reading: it required a logger throw to discard a
queued rerun, but actual runReopenResync schedules in finally. Executing the real
source produces two fetches when live and one when disposed. The maintained
listener oracle now extracts and executes that actual function, capturing the
original unhandled rejection and proving finally runs first. A passing test
written from a reading did not prove source behavior; this is why the live case
exists. The second gap was response metadata on a non-ExchangeError subscriber:
opaque recovery remains first, but its fallback must retain canonical response
so the overlay catch selects response.data.msg. Both repairs require incremental
verification/review; the earlier passing candidate is not approved.

The review-repair controls are scoped independently: early return after reporting
the original logger error compiles and fails `source finally must retain queued
resync after logger throw unless disposed`; removing only the fallback response
compiles and fails `canonical subscriber response must survive non-ExchangeError
projection`. Both mutations were restored. The first attempted scheduling mutant
also removed reporting and failed that assertion first; it was narrowed before
being accepted as a scheduling control. Restored verification passes 70 library
tests, five real-Store notification tests, seven cancellation inventory tests and
five override tests, all with include-ignored. The inventory retains all 115
creators and both snapshot dispatch calls; its resync witness is the normal
runtime test, while the separate actual-source oracle runs in mandatory CI.
Local extractor reports zero updates/additions/orphans.

Review-repair strict all-feature/all-target host Clippy and wasm Clippy pass
separately with one job; parity passes 7334 rows / 2801 public items. Exemptions
parse as 359 blocks / 359 distinct / zero blank reasons. Incremental adversarial
review and the final integration merge remain pending.

### Notification integration validation

Candidate-repair review approved 9a5c25e0a9 after independently running the
source-finally comparison and five notification tests. Merge 338888e38a takes
integration 25f524cd7c; its only conflict was numeric-ingress documentation,
resolved as a union. The automatic numeric-ingress merge retains both the
PositionRecord registration and integration's canonical retained capture.
191 core tests plus two doctests pass with zero ignored. The following merge
takes 5d55c8ec6e with no core, transport or lockfile change. Final-tree strict
host and wasm Clippy pass separately, parity passes 7334 rows / 3259 public
items, local extraction has zero updates/additions/orphans, layer tests 78 and
CI tests 32 pass. CI steps 89/90 become 90, then 90/90 remain 90, with none lost;
real TOML parsing gives 359 blocks / 359 distinct / zero blank reasons and no
parent entry loss. Normal/dev/build/target dependency tables were checked;
Bitget's sole removed dev table is its incoming-parent Store dependency moving
to production, retained exactly. Final merged-state review is still pending.

Separate pending seams are not closed by notification fallibility: response
provenance remains on its own reviewed branch; native non-finite query numbers
need query representation rather than JSON projection; raw numeric/object
emitter inputs remain unrepresented by ExchangeErrorInput's Message/Error/Missing
variants (reported by the validation-type owner). Venues must not coerce those
inputs early and claim source catch/coercion behavior is reproduced.

### Restored-CI override diagnosis

The unchanged override test was compiled against an isolated git archive of
a3d514bec8 (reusing the exact quote/syn/serde_json artifacts from the scoped test
build), and failed with exit 101. Discovery found 78 rows against 75 expected:
no existing row was removed or changed, and exactly three calls were absent:

- safe-cex-venue-binance/src/public_dispose.rs::dispose_binance_public,
  body 0508f77b38489d4b: socket.dispose() targets an explicitly typed
  BaseWebSocket after dispose_market succeeds, not a BaseExchange bypass.
- The same file's dispose_binance_pm_public, body 409619d548f15a15:
  the identical typed socket cleanup sequence.
- safe-cex-venue-bybit/src/ws_private.rs::send_reconnect_sequence,
  body e32678aff8d5d26e: owner.emitter().fetch_and_set_private_snapshot(1)
  dispatches via WsParent and its future enters start_message_operation.

All three source bodies are byte-identical between integration and this
notification branch. This branch already records all three from the reviewed
notification inventory reconciliation; its unmodified gate passes freshly. No
new inventory update was needed to repair the integration failure, and no
Exchange override bypass was admitted. The archive-only discovery was restored;
it did not edit the working-tree inventory.

### Fixture identity and numeric capture sweep

The sub-agent audit triaged 150 owned files. Its initial bounded lexical scan
covered 101 files and 2,530 record windows, finding 81 equal-literal candidates:
43 had differentiating sibling witnesses, 12 made no field-identity claim, six
were scanner/cleanup state, and 20 belonged to three repaired fixture groups.
The other 49 files were classified individually: 23 production files without
test modules, 11 compiler goldens, six control drivers, three documents, four
path-included test files and two generated feeders. Manual inspection also
covered nested, computed, helper-built and default fixtures. This is a complete
file/surface triage, not exhaustive proof against every possible field swap.

The three fixture repairs distinguish reporting testnet/publicDataOnly flags,
amount/price precision in both caller and minted-ID placement, and all six
subscription Candle fields. The reporting golden retains its original 18 cases
and bytes; a live actual-source supplement adds 36 differentiated cases with
both counts asserted. Its only discovered consumer is the core reporting test. The Candle test reads
native fields before serialization. Five additional gate groups now assert
native values: WS signing envelopes, HTTP signing envelopes, secondary options
(nine fields across 729 cases), remote-stage config, and health enums/state.
Health remains a single-entry corpus covering 11 categories, not Network or
multi-entry ordering. No claim of those missing behaviors is made here.

Compiling mutations failed these named assertions, then were restored:

- `source error report preserves testnet/publicDataOnly field identity`
- `caller split uses amount precision, not price precision`
- `mint split uses amount precision, not price precision`
- `subscribed candle native open identity`
- `native primary key identity`
- `native WS toSign identity`
- `native HTTP method identity`
- `native remote method identity`
- `native health category identity`

The Candle, primary-options, HTTP and health mutations swap serialization and
decoding names together: a symmetric roundtrip can cancel their corruption,
while the native assertion fails. The remote oracle is explicitly run with
include-ignored; an initial zero-execution control was rejected and rerun.
Restored focused checks pass 61 exchange and 42 transport tests, zero ignored.

Four numeric bit-capture/assertion files were audited: transport
try_parse_json_numbers.rs, its capture.cjs, parser_signed_zero.rs, and core
numeric_ingress_findings.rs. Their closed JSON lexeme sets cannot produce NaN;
zero corpora needed NaN canonicalization. Signed-zero and finite-bit corruption
controls compile and fail `signed-zero capture retains its observable sign`
and `non-NaN capture retains every payload bit` respectively. The old parser
golden bytes remain unchanged; a provenance supplement records its unknown
original architecture and a fresh exact verification on darwin/arm64 Node
v24.20.0. The source capture now refuses an unexpected NaN. Response-provenance
branch corpora are outside this tree and must be audited when that ABI is taken.

These fresh source checks used the root-installed TypeScript and source
dependencies. They do not claim a newly performed isolated frozen install.

The final full core run passed 192 tests and two doctests, zero ignored. The
subsequent retained-golden/live-supplement refinement passed all four reporting
tests, and both its compiling flag-swap and missing-metadata controls failed by
name and were restored. All-target/all-feature strict host and wasm Clippy
passed separately. Layer checks passed 78 tests, CI wiring passed 251, extraction
reported zero updates/additions/orphans, and parity passed 7334 rows / 3259
public Rust items. The sub-agent approved the merged code and the incremental
corpus refinement. Its independent parse retained 90 CI steps and 359 exemption
blocks / 359 distinct / zero blanks, with no parent entry or dependency loss.

### Batched oracle process audit

The load-policy audit inspected 34 literal Node launch sites across 28 owned
Rust test/source files. Two corpora launched per case: proxy numeric ingress
made three Node calls and tunnel staging made four. They now make one each,
seven calls reduced to two. Numeric output preserves ordered string bit
patterns with a cardinality assertion. Staging loads the actual source in a
fresh VM for each flag and passes each two-region result to its corresponding
Rust child; four native children remain necessary for process-global endpoint
initialization. Both focused tests pass after batching, zero ignored, and the
sub-agent approved flag/payload ordering and environment isolation.

Other apparent loops are independent corpora: parser numeric/boundary scripts,
plain/bigint parser modes, and proxy-token modes each invoke Node once per
corpus. No other per-case Node launch was found. This audit does not remove
source executions or weaken case counts to reduce process load.

### Response merge after notification PR 7239

The response branch merges notification candidate af44700b07 without changing
that PR branch. Both reviewed histories remain intact. Conflict resolution keeps
new KuCoin raw-body fixture selection inside explicit Encoded construction and
migrates its newly added ticker model reader to json_value; no source response
parser is implied by that accessor. Real TOML parsing gives response parent
332/332, notification parent 359/359, and merged 364/364 blocks/distinct, zero
blank reasons and no parent target loss.

Core and transport test targets compile on the merged tree. The six-venue check
stops at notification-ABI callers in Binance public_start (log and Store.update)
and spot_open (emit_and_log_error). Those failures require venue-owned outcome
propagation; this response change does not suppress them or claim all consumer
targets compile. Merged response validation and review remain in progress.

The five response exemptions are RestResponseBody itself and its into_data,
transform, try_transform and json_value operations in transport/provider.rs.
They expose the host-adapter provenance contract; they are not claimed standalone
TypeScript declarations. The original response commit introduced them; this merge
retains them alongside every notification exemption. Response model tests pass
3/3, and the separate RequestDump.body finding still reproduces lexical -0 loss
with the newer ingress dependency (1/1 focused test), so it remains unresolved.

The keep-going six-venue compiler collection reports 35 unique sites (repeated
lib/lib-test diagnostics deduplicated): 19 unused Results, 12 outdated WsParent
methods, three notification type mismatches and one response assertion. The
Bybit response assertion is migrated here. Remaining sites are venue-owned
notification cutover work; the check is RED-ON-UNMIGRATED-VENUES.

- `safe-cex-venue-binance/src/public_start.rs:22`: unused `Result` that must be used; `base.log`.
- `safe-cex-venue-binance/src/public_start.rs:27`: unused `Result` that must be used; `base.store.update`.
- `safe-cex-venue-binance/src/spot_open.rs:49`: unused `Result` that must be used; `socket.emit_and_log_error`.
- `safe-cex-venue-bybit/src/ws_private.rs:66`: unused `Result` that must be used; `parent.emit`.
- `safe-cex-venue-bybit/src/ws_private.rs:110`: unused `Result` that must be used; `owner.emit_and_log_error`.
- `safe-cex-venue-bybit/src/ws_private.rs:116`: unused `Result` that must be used; `owner.reconnect`.
- `safe-cex-venue-bybit/src/ws_private/local_auth.rs:88`: unused `Result` that must be used; `socket.emit_and_log_error`.
- `safe-cex-venue-bybit/src/ws_public.rs:45`: unused `Result` that must be used; `parent.emit`.
- `safe-cex-venue-bybit/src/ws_public.rs:113`: unused `Result` that must be used; `socket.emit_ws_diagnostic_send`.
- `safe-cex-venue-bybit/src/ws_public.rs:118`: unused `Result` that must be used; `socket.emit_ws_diagnostic_receive`.
- `safe-cex-venue-bybit/src/ws_public.rs:187`: unused `Result` that must be used; `socket.track_last_received`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:200`: method `log` has an incompatible type for trait; `fn log`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:201`: method `emit` has an incompatible type for trait; `fn emit`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:204`: method `sync_socket_health_to_store` has an incompatible type for trait; `fn sync_socket_health_to_store`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:205`: method `update_latency` has an incompatible type for trait; `fn update_latency`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:208`: method `emit_ws_reconnected` has an incompatible type for trait; `fn emit_ws_reconnected`.
- `safe-cex-venue-bybit/src/ws_public/tests.rs:209`: method `emit_and_log_error` has an incompatible type for trait; `fn emit_and_log_error`.
- `safe-cex-venue-bybit/src/ws_public/tests/exchange_composition.rs:270`: mismatched types; `br#"{"retMsg":"venue rejection"}"#`.
- `safe-cex-venue-kucoin/src/accounts.rs:24`: unused `Result` that must be used; `base.emit_and_log_error`.
- `safe-cex-venue-kucoin/src/order_reads.rs:63`: unused `Result` that must be used; `base.emit_and_log_error`.
- `safe-cex-venue-kucoin/src/spot_helpers.rs:57`: unused `Result` that must be used; `store.update`.
- `safe-cex-venue-kucoin/src/spot_reads.rs:73`: unused `Result` that must be used; `base.emit_and_log_error`.
- `safe-cex-venue-kucoin/src/spot_reads.rs:217`: unused `Result` that must be used; `base.emit_and_log_error`.
- `safe-cex-venue-mexc/src/exchange.rs:132`: `match` arms have incompatible types; `Err`.
- `safe-cex-venue-mexc/src/exchange.rs:217`: `match` arms have incompatible types; `Err`.
- `safe-cex-venue-mexc/src/exchange_tests.rs:128`: mismatched types; `Box::new`.
- `safe-cex-venue-okx/src/public_socket.rs:131`: unused `Result` that must be used; `socket.track_last_received`.
- `safe-cex-venue-okx/src/public_socket.rs:137`: unused `Result` that must be used; `socket.handle_pong_event`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:230`: method `log` has an incompatible type for trait; `fn log`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:231`: method `emit` has an incompatible type for trait; `fn emit`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:232`: method `sync_socket_health_to_store` has an incompatible type for trait; `fn sync_socket_health_to_store`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:233`: method `update_latency` has an incompatible type for trait; `fn update_latency`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:234`: method `emit_ws_reconnected` has an incompatible type for trait; `fn emit_ws_reconnected`.
- `safe-cex-venue-woo/src/../tests/support/ws.rs:235`: method `emit_and_log_error` has an incompatible type for trait; `fn emit_and_log_error`.
- `safe-cex-venue-woo/tests/orders.rs:7`: unused `Result` that must be used; `Store::update`.

A current RestResponse census adds Hyperliquid to the original six venues. Its
reader now preserves native model data and the existing empty-encoded-to-Null
case, and all six known constructors select Encoded explicitly. This is a
compiler-forced response migration, separate from notification error handling.

Hyperliquid test targets compile after that migration. The focused whole-HOW
native-data test passes (1/1). Static merged-state review verifies CI 87/90
parent named steps become 90 with no loss, no dependency/lock/layer-policy delta
against notification, and approves the union for commit. It does not declare
integration readiness: venue notification migrations and final gates remain.

A fresh skip-Transformed-parser mutation compiled but initially reached an
unnamed as_f64 unwrap before the expected assertion. That failure was rejected
as named-control proof. The test now asserts the native output type before its
existing exact numeric bits. The same production mutation compiles and fails
`HOW second parser must see the decoded string`; restoring production passes
1/1. Sub-agent review approves this assertion-only follow-up.


### Store subscriber identity: independent notification follow-up

This repair is based on integration 9419b340d8a6, independently of response
provenance. The pre-existing ExchangeError conversion discarded Store-local
subscriber IDs on both opaque-cause recovery and fallback projection. It now
appends each notification ID inner to outer, retaining the original metadata,
canonical fallback response and unchanged cause. Equal IDs from different Stores
are not globally identifying. External exhaustive metadata literals gain a field.

Native assertions pin nested IDs 17/42, fallback 91, the unchanged original,
metadata/response retention and no invented ID on non-notification errors.
The origin-only position failure still leaves shared fetch waiters successful.
The two scoped targets pass 20 tests with zero ignored, including the source
listener-order oracle. Independently clearing the recovered or fallback list
compiles and fails respectively `recovered ExchangeError retains inner and outer
subscriber identity` and `projected subscriber error retains subscriber identity`.
Neither a response representation cutover nor venue call-site migration is claimed.

Fresh strict Clippy passes separately on the host and wasm32-unknown-unknown
(all targets and features, locked, jobs=1). Whole-workspace fmt passes. A real
tomllib parse retains 359 exemption blocks / 359 distinct / zero blank reasons;
this slice changes no exemption, manifest, dependency or response representation.


### Response consumer collection after the landed notification cutover

At integration 9af82bfeae, the second keep-going check covers seven venues plus
core and transport test targets. It exits 101 with 24 distinct primary sites:
Bybit 7, OKX 5, MEXC 3, WOO 7, and two new KuCoin response-reader mismatches.
Binance and Hyperliquid produce no diagnostics in that pass. This is a floor:
earlier type errors prevent later unused-Result lints from being collected.

After the response-only Spot cancellation model-view migration, the third,
KuCoin-scoped check clears both response type errors and exposes six unused
Results: accounts:24, order_reads:63, spot_helpers:57, spot_reads:73/217, and
spot_writes:81. These are the owner's six notification sites, including the new
cancel emitter; no suppression or successful-unit bridge is introduced here.
Full consumer validation remains RED-ON-UNMIGRATED-VENUES, not complete.

The merge retains all 90 named CI steps and every complete exemption entry from
both parents (364 blocks / 364 distinct / zero blank reasons). Subscriber-ID
runtime code is identical to both parents; incoming dependency and lock changes
match integration. Static sub-agent review approved the union and the separate
Spot response-reader change, without claiming consumer readiness.

Fresh merged-state response checks pass: three transport provenance tests, the
one whole-HOW native-payload test, and Binance's live-source public factory
witness (22 direct/HOW cases, one batched source process). Node 24.20.0 on arm64
uses the existing root dependency installation; this is not a fresh hermetic
install claim. No full consumer pass or final response merge readiness is claimed.

### Pending Store mutation variant and opaque-cause projection

FINDING: subscriber-ID retention does not preserve the StoreMutationError
variant or an arbitrary opaque subscriber cause at ExchangeError::from.
ExchangeError's Error implementation currently has no source; the fallback
projects message, response and IDs. Recovering an ExchangeError preserves its
metadata but still does not expose the outer StoreMutationError variant.
Binance PR 7254 owns the six native cases across three wrappers/two cause paths
and variant/pointer controls. Its original-error box is not redundant while
this shared projection remains lossy. Core owns a separate structural repair;
response provenance does not claim to close it.

### Response candidate acceptance after frozen integration 710e663156c8

Production gates ran on 3ae5397cd4; the subsequent 3ceeab861c changes only four
pending recovery notes to open with FINDING, preserving all parsed row fields
except those note prefixes. Core passed 194 tests plus two doctests; transport
passed 157 tests plus six doctests: 359 passed, zero failed, zero ignored, with
include-ignored. Strict host and separate wasm32-unknown-unknown Clippy passed
for both crates, all targets and features, serially with one Cargo job.
Whole-workspace rustfmt check passed. Layers passed 78 tests. Parity passed at
7339 rows / 3327 public Rust items. A real tomllib parse measured 364 exemption
blocks / 364 distinct targets / zero blank reasons, with every complete frozen
integration block retained; the five response additions are justified individually.

A fresh compiling production mutation skipped transform on Transformed data.
It failed the exact assertion `HOW second parser must see the decoded string`;
restoring production code passed all three response-provenance tests. This
supplements the earlier four controls rather than relabeling their historical
runs. Source captures use Node 24.20.0, arm64, the existing dependency install.

Extractor check remains inherited red: three manifests would update, two added
chaser symbols and zero orphaned symbols, plus BingX API formatting drift.
No core response manifest drift was reported. The seven-venue consumer check
remains RED-ON-UNMIGRATED-VENUES as inventoried above; no full consumer-green or
integration-ready claim follows from the scoped core/transport pass. KuCoin's
migration-only reviewed ancestry includes uncleared sweep work and was not
merged merely to silence this check.

The pending-versus-divergent audit parsed 523 owned rows across 37 manifests:
zero divergent rows without an rs target. Four absent recovery operations stay
pending with explicit FINDING notes and call-site inventories; no Rust targets
were fabricated and no status or denominator changed in that note correction.

## Shared journal pipeline — f7ac999356 / 7535bf14d8

Built on 3bd38d6336, retaining inherited unreviewed WIP 52f664284e in
ancestry. f7ac999356 implements interval reconstruction, fold projection and
standalone buildRecords; 7535bf14d8 corrects only a control assertion selector.
Python 3.11 tomllib against integration 2bc83d78d4 measures this slice moving
6 pending->ported and 8 pending->divergent. Committed owned Bybit+six-journal
scope cumulatively moves 6/13, with 114 ported / 42 divergent / 388 pending.

On 7535bf14d8:
- Scoped safe-cex-exchange all-target cargo check --jobs 2: exit 0.
- Two live journal tests after exact restoration: exit 0, 2 passed, zero ignored.
- Compiling missing-reducing-PnL mutation: exit 101 at JOURNAL native absent
  interval realizedPnl. Compiling reversed-lookback mutation: exit 101 at
  BUILD native lookback. The runner exited 0 after restoring both files.
- Sub-agent review of committed production/tests/oracle: no remaining production
  defect; its source-only run exited 0. It ran no Cargo or mutations.

Enumeration floors: 192 interval scenarios stopped at 191, four direct mapper
cases stopped at 3, twelve async build scenarios stopped at 11. Assertions check
native fields and number bits, native optional absence, callback arguments,
independent source cardinalities and complete comparison totals before any wire
comparison; no native record serialization is used as the equality oracle.

Review found representable bare-leg seed behavior (source slice(-1)) and it was
corrected with a source/native witness. The initial build harness created errors
in a foreign VM realm, so instanceof Error suppressed their name; using the
source realm's constructor corrected the fixture, after which native name/cause
assertions passed. An initial control selector expected the income lookback
assertion, but the earlier account-wide fills assertion fired; the runner
rejected that selector, now corrected in 7535bf14d8.

Extractor writer/check both exited 0, zero additions/orphans and zero remaining
drift, but these runs included six uncommitted venue-adapter manifest drafts:
this is not labeled a clean-7535bf14d8 extractor result. Repeat on the final
committed delivery tree. Rust parity indexer exit 1 has 24 inherited diagnostics
(AsterDEX/WOO renames and three core/AsterDEX inventions); no journal or Bybit
mapping diagnostic. It is not a global parity-pass claim.

Not run: full core tests, workspace compilation, host/wasm Clippy, CI or monorepo
suites. Typed callbacks/read rows/error carriers and native lazy futures retain
explicit CHOSEN boundaries. V8 comparator schedules for non-transitive mixed-ID
ties are not claimed. Existing JournalFolder binding and venue fetch composition
remain outside these standalone symbol mappings.

## Native scalar JS Store patches (ddi2il-43)

A real Bybit Inverse private position frame produced NaN leverage. The old JS
facade converted it to JSON null, causing updatePositions to reject the patch and
retain leverage 10. The independent shared fixture reproduced this through the
production facade and DefaultStore: updatePositions NaN leverage and
updatePosition NaN entryPrice both failed at runtime before the repair.

`tests/js_store_patch_numbers.rs` now passes three tests: seven native leverage
bit cases through updatePositions, 192 individually patched values covering all
32 scalar numeric fields on updatePosition/updateOrder/updateTicker/updateMarket,
and omission/null/unknown/opaque-info/update-only assertions. The repair uses
`to_typed::<f64>` from the existing native-number bridge and
`ModelPatch::set_number`. Opaque info and socket health retain their explicit JSON
boundaries; nested option terms retain their existing native typed decoder.

`tests/js_store_patch_numbers_controls.py` runs sequential compiled faults:
replacing the native carrier with JSON projection, and normalizing signed zero.
Each compiled, failed both named native numeric regressions, then passed all
three tests after exact byte restoration. The source controls do not count a
compiler error as regression evidence.

The Inverse branch checkpoint 84b572c4c7 stacks this fix onto its separately
staged facade. All nine `inverse_binding` E2Es pass, including the real private
socket order/fill/position delivery that previously failed NaN leverage. This
validates the shared repair through that construction and socket path; it does
not claim the remaining Inverse binding closure work is complete.

Final scoped validation: js_runtime 68, typed_js_values 5, new patch regressions
3, override_contract 6 and cancellable_future_inventory 6 pass. The runtime and
override suites include their maintained-source tests; only the existing
cancellation discovery export remains ignored. Host all-feature/all-target and wasm library
clippy pass; parity has zero findings and extractor reports zero changes.
After merging the separately owned BreakoutProp classification repair #295,
the override inventory export passed. Review found exactly one changed enclosing
body fingerprint among 1164 retained rows: Store facade `35d694b8312ceda6` to
`c6140fadf0482da7`. Its existing Store `update_order` call is unchanged; the
facade now selects the JSON-only socket-health helper explicitly. No call was
added or removed, and no delegation policy or exemption changed.
