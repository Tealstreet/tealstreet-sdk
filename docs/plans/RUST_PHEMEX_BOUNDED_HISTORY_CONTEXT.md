# Phemex bounded REST history custody

Plan first, 2026-10-24. Owner codex-ih35js; overseer claude-ddi2il.
Branch `feat/rust-phemex-bounded-history-context` starts at actual landing
`15bd7821b52e378a9e32a60ecb897275c52109a4`. This phase adds this plan, its index
row and a new original-source test target only. No production or manifest edits.

## Selection and ownership

The fresh open-PR file inventory contains no Phemex source/test claim. The
largest remaining request file, lib.rs (17 fingerprints/18 occurrences), mixes
transport, sockets and multiple consumers. This slice instead selects all of
history.rs: bounded fills, closed orders, public trades and client-ID lookup.
It has six fingerprints/nine occurrences, larger than identity.rs (3/3),
account.rs (3/3), income.rs (2/2) and trading.rs (4/4).

Whole Phemex currently has 35 fingerprints/39 occurrences. The full later
history conversion targets 29/30, with every other path map retained. Five of
the nine matches are conservative macro references to the binding `request`,
not physical calls. They may retire with the direct managed-read binding
cutover while preserving the expressions and all assertions. Do not describe
nine lexical occurrences as nine requests or weaken the scanner.

codex-5c57jl is unparking whole-family Phemex sockets from `254f55689a7` onto
frozen #1943. It explicitly cleared history.rs and the new
tests/request_context_bounded_history.rs. Socket codec, Connection, public and
private transport/auth/subscription/timer hooks and socket tests stay its scope.
Current plan/RED work edits none of those files. Future lib.rs edits, if needed,
are confined to Pending::History response/failure and typed delivery guards;
the socket owner confirmed that boundary. Keep the common REST funnel and all
other callbacks intact, and coordinate the exact hunks before implementation.

## Exact target inventory

| Fingerprint in venues/phemex/src/history.rs | Count | Current expression | Destination |
| --- | ---: | --- | --- |
| `007e1c20fd9ad3fe2cb38a2646b5f0910bcced3498841d2ec9362b0da3b4e4f8` | 1 | self.request(... Pending::History(request), Some(at)) | One managed Read attempt under genuine command entry |
| `aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f` | 1 | cx.done(request.command, data) | Exact original Read completion for lookup |
| `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 2 | cx.fail(request.command, error) | Exact original Read failure for lookup |
| `139c5990615d19868461cf9c3fc7c97d895017fced401b219198883e89eeefd1` | 3 | matches!(request.rows, Rows::Trades(_)) | Preserve predicate during managed binding cutover |
| `a25ed19569db55f160c6e07637c3dfe1a7151e2722da2780fb332e3fe3cab917` | 1 | format!("symbol={}", request.native_symbol) | Preserve native query bytes |
| `f913fd4fec0b8ab4a85e00f3b7439983491def6ad5bc264f606d18cbfc7da52e` | 1 | format!("{time}:{}", request.count - 1) | Preserve duplicate public-tuple identity |

Legacy HistoryRows terminals do not appear in this lexical map, but their
Completion::new(command) authority must also move to ReadCompletion and guarded
ReadHistoryRows. Removing only the visible numeric lookup terminals is not a
complete adoption. Existing local refusals already use finish_local_history;
preserve their decision order and native messages from #1917.

## Native request and result contract

Each selected query makes exactly one physical GET, no pagination or retry:

| Query | Endpoint | Body selector and native query |
| --- | --- | --- |
| Fills | /api-data/g-futures/trades | Root array; symbol, optional start/end, offset=0, limit |
| Closed orders | /api-data/g-futures/orders | Root array; same bounded query |
| Client-ID lookup | /api-data/g-futures/orders/by-order-id | Root array; symbol and encoded clOrdID |
| Public trades | /md/v2/trade | result.trades_p; symbol only |

History has at most 16 active query IDs; duplicate active IDs are refused before
starting work. Private credential availability precedes normal symbol/bounds
validation. Lookup first checks client ID, then unknown market returns the
non-definitive sentinel, before ordinary private checks. Input bounds are
nonnegative safe integer milliseconds, ordered when both supplied. The native
symbol accepts ASCII alphanumeric/underscore only. Fills, orders and public
trades default to 100 output rows and clamp 1..200; lookup fixes its limit to 1. The public endpoint
has no size parameter and separately refuses over 1000 response tuples.

Keep exact nanosecond division before conversion, millisecond filtering, current
fill timestamp units, strict textual enums, fees, native quantities, closed-only
order filtering, last result sorting/truncation and execution/order deduplication.
Equal public tuples remain separate with occurrence-based IDs. Lookup accepts
one matching symbol/client-ID row and yields definitive absence only from a
successful empty response; malformed/oversized/native/transport errors reject.

Both direct and streamed delivery validate the entire response before accepting
the staged page. A failed selected query is strict: no partial prefix, fabricated
empty success or incomplete marker. A malformed later streamed row discards the
earlier staged row. Rare history never mutates live balances/orders/settings.
The income range owner, USDT/USDC stages and partial-recovery contract are outside
this slice, as are account repair, startup, trading and all sockets.

## Shared authority and prerequisites

Use genuine admit_http_command with Operation::Read, a declared 20000 ms root,
RetryPolicy::Never and the selected credential. Admit before history.active
insertion or any deferral timer; rejected admission consumes no native slot.
Bind one original ReadCompletion; admit every streamed item and final payload
through exact shared delivery validation before row mutation or completion.
The History delivery gate must precede common lib.rs response side effects,
including 429 paused_until mutation and envelope-error routing. Terminal cleanup
must match the retained root/generation before removing a reused numeric ID.
There is one response,
no command-wait propagation, group G1/G2, Plan, execution join or session owner.
No #1945 stack, feature or root API is needed for this bounded slice.

The root bounds existing pause/route/signing preparation as well as the request.
Native history_send defers on paused_until with an existing timer: keep that
timer, validate the original root when it wakes and never renew it or send after
expiry. Queued cleanup is observed at that native wake; this plan does not claim
an automatic exact-at-root queue timer. Record the bounded-wait change in CALLERS
with implementation. Released/parked deadline failures and preparation/network
errors preserve their typed kind/metadata; do not infer type from text. Native
transport failures currently lose their type: Network/Timeout preservation is
a new RED requirement. Non-lookup HistoryRows already enriches untyped errors
to Internal; adopting ReadCompletion extends that enrichment to lookup errors,
whose native numeric failure currently has no kind. Declare this caller delta
with implementation and preserve all preexisting typed fields. Initial Read
admission refuses InvalidInput/Internal with outcome None. Duplicate,
foreign, late and reused-ID callbacks grant no row or completion authority.

Ordinary GETs select Secondary when present, else Primary. Client-ID lookup is
Primary and preserves explicit primary routing. Public trades retain optional
authentication when credentials exist, even for public_data_only. An unsigned
path is eligible only when both constructed credential containers are absent.
Do not silently make a credentialed public read anonymous.

claude-4yjlw0 confirmed no reviewed Phemex dispatch SignStep recipe exists and
Phemex remains locally signed at this base. The selected-read helper's reviewed
older pin is f1671230ce8; newer 74d0fef3ff7 is not cleared or landed. Neither is
consumed here. Signed implementation waits for an assigned Phemex recipe and
the reviewed landed helper. No third local signer/read wrapper, raw credential
Context, copied HMAC recipe or legacy signing fallback is permitted. Pure
unsigned public trades can use request_managed, but that alone does not close
the full six-fingerprint target. Plan/RED work is unblocked; signed cutover is not.

## Original REDs and controls

Commit this plan before a standalone real Engine<PhemexPerpetuals> target. Keep
production, existing fixtures and all inventories byte-exact to the pinned base.
Use monotonic clocks and the existing test_support::prepared_actions transport;
do not add a fake Context, timer driver, issuer, signer or numeric fallback.

Required original assertions: public/private/Primary-lookup paths expose managed
preparation instead of physical I/O; preparation cannot release data or complete
early; the original 20 s deadline and typed Timeout survive late/reused-ID input;
network failures retain Network; exhausted shared Context capacity refuses the
read before I/O. Add actual native pause longer than 20 s, preparation parked
across expiry, native16-slot recovery after terminal cleanup, and stale429 that
cannot extend the pause. Expiry must neither send nor yield lookup absence.
Count only compiled assertion failures as REDs, never fixture
or compile failures. Preserve positive native controls for exact public tuple
identities/values, strict malformed-after-valid streamed rows, private signing
and Primary/Secondary selection, lookup absence, duplicates and active-capacity
recovery. Use the same response bodies for accepted and denied publications.

Independent source/adversarial, caller and exact-map reviews precede production.
Architecture gate: No venue lifecycle, retry, signing or outcome algorithm is
introduced; this adapter projects native codecs into the existing shared core.
Missing shared signer APIs stay explicit prerequisites. This task stops at
plan and original REDs. No manifest prune, managed-read or CI-green claim yet;
implementation, focused Phemex/Binance gates, both request checks, independent
review and exact-head CI follow authorization and dependency clearance. No merge
without a train window.

## Original-source checkpoint

Plan b3c7d479423 and review corrections 6eadc16132c precede the original test
commit ef8156dc53d. Test-only review closure c072d117341 adds optional public
authentication coverage and strengthens peer/early-response custody checks.
The qualifying unchanged-production run compiles 15 tests: ten expected
behavioral assertion REDs, five native compatibility passes, zero ignored.
The initial preparation-method compile typo and unequal snapshot-header clocks
were fixture corrections before that run; neither is counted as a RED.

The new target's strict Clippy and rustfmt pass. Both request-context checks pass,
including all 15 checker controls. Independent source/adversarial, caller and
full-map reviews are CLEAR at c072d117341. Only this plan, the README row and
the new test target differ from actual15bd; production, existing fixtures,
engine, scanners and all manifests are byte-exact. This closes the requested
plan/RED phase only. No source adoption, inventory reduction or CI-green claim.

The overseer assigned the Phemex recipe migration to codex-zs5e83, which is
planning from actual15bd. It owns sign.rs/provider/setup and coordinated request
funnel plumbing; history remains this lane's. The exact selected-read and native
byte requirements above were sent before any implementation. No reviewed recipe
or landed helper is consumed by this checkpoint.
