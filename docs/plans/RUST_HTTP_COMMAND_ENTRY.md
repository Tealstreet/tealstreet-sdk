# Engine-owned HTTP command entry and pre-authority rejection

Plan first, 2026-10-23. Base actual SafeIDs #1789
7641650e7b1fc54c55593880ac16299171f60a90. Owner prioritizes this small
shared prerequisite before live whole-plan admission: BloFin and Lighter public
candle migrations cannot remove their last numeric issuance-refusal fallback
while http_context can fail before a Context exists. The Binance reference has
the same boundary. Background-session Context is owned by claude-55ao7t and is
outside this change. Local Plan WIP5d30986a65e stays on its separate branch.

## Contract and API

Add Cx::admit_http_command(Policy) -> Result<Context, CommandEntryError>, with no
numeric command argument. CommandEntryError::Rejected(Error) means the engine
already published exactly one correlated terminal failure for this unissued
command. The adapter returns without calling numeric fail. Revoked means there
is no exact fresh command-entry custody (outside callback, repeat use or an
already-owned command); it grants no rejection/completion rights for any owner.
The returned Context is the existing registry-issued type, with unchanged
credential intent, raw deadline, generation and managed HTTP lifecycle.

The Engine lends private entry state immediately around the actual
Venue::on_command/on_venue_action callback for completion-bearing commands, after
normal duplicate/deferred/capability guards. Its owner is taken from that engine
command event, never a caller-supplied numeric identifier. It is initial-only:
existing read legs, retries and pending callbacks retain their Context APIs.
Native callbacks cannot clone/export a loan or use a foreign/stale command ID.
A repeated call after either success or rejection refuses without another notice
or resource allocation. A failure never fabricates a Context, journal, timer,
signature, accepted write fact or fallback owner.

Before calling the existing http_context implementation, validate fresh entry
and absence of an already-live managed owner. On success mark it bound. On a
policy/time/registry/identity refusal mark it rejected and publish through one
engine rejection transaction, without invoking numeric Cx::fail or the legacy
numeric ContextRegistry::seal bridge. Use the original typed Error for the
returned reason. Classify Invalid as InvalidInput, TimedOut as Timeout and
capacity/state failures as Internal; a Mutation refused before ownership has
Outcome::NotSent, while Read failure carries no financial write outcome. Retire
only this command's admission/active order-lock state, preserve unrelated owners,
and do not clear sent evidence or pending transport resources of any owner.

The entry loan must close at the real native callback return. A caught callback
panic must permanently fence entry reuse, delayed provider/permit work, event and
action drains and Venue-bearing query methods. Preserve existing allocator-only
query compatibility. No temporary HTTP-delivery marker removal can create an
entry loan: only a genuine command callback receives it. Nested/deferred command
callbacks receive their own actual event owner and cannot inherit another one.
The shared API must also work on a real command delivered after earlier managed
commands have completed, including reused numeric command IDs.

## Scope

Engine: new request_context_entry.rs service, public error type/export in
request_context.rs, private Io entry state and narrowly scoped callback/query
fences in engine.rs. Reuse existing registry issuance and order-lock cleanup;
no second context registry, allocation counter, signing provider or outcome join.
Binance: remove numeric refusal handling only at its initial read/trade context
admission boundaries, retaining native plan/input errors and actual HTTP/parser/
retry algorithms. Background reads and session upkeep remain unchanged.

This does not solve cached completion before any admission, arbitrary native
validation errors, background session ownership, same-call retry backoff,
whole-plan roots/reservations or joined observer completion. Those remain
explicit separate contracts; it is not a claim that any family is fully zero.
No venue-local completion wrapper, broad ratchet exemption or hidden numeric
helper may be introduced. Inventory pruning/rehashing follows the standing rule:
never grow the total, record exact old/new maps and independently audit every
unaffected map byte-for-byte.

## Verification and review gates

1. Commit this concrete plan and audit before implementation. Verify actual
   command entry placement (including Venue actions, deferred order commands,
   unsupported command guards and command-ID reuse) before choosing the fields.
2. Assertion-failing architectural fixture: a real command encounters initial
   Context refusal and returns without the legacy numeric fallback; no terminal
   notice exists. Preserve its clocks/policy/capacity and exact completion count
   while switching to the shared entry API. This demonstrates the missing seam,
   not a newly discovered production regression in the old numeric fallback.
3. Engine controls: registry capacity, safe-ID capacity, invalid time/policy,
   successful Read/private Mutation, NotSent before write, exactly once after
   repeated use, outside/HTTP/public/timer callback refusal, another active owner
   unchanged, no new Context/request/timer/provider on refusal, reused ID and
   genuine deferred command owner, caught-panic no later custody/query/drain.
4. Actual Binance native candle/trade admission controls plus existing native
   suites retain all normal outputs and unsent/partial/outcome assertions. Send
   reviewed API/PR pin to BloFin and Lighter for their separate native adoption.
5. Full engine and scoped Binance native tests, strict all-target/all-feature
   Clippy, formatting, ban/ratchet controls and nearby CALLERS/plan/index updates.
   Independent source and full inventory-map audit before source freeze.
6. Draft PR/CI watch and complete fresh proof under standing source/artifact and
   baseline rules. Real-integrate any newly landed shared/build dependency.
   No guard or merge until the owner grants the landing window; guard and merge
   remain separate commands with exact reviewed head and audited resulting tree.

Plan audit refinements before implementation:

- Successful terminal publication consumes current same-command entry custody,
  including legacy done/fail and Context completion. New admission cannot follow
  an earlier result in the same callback. Conversely, automatic Rejected blocks
  a same-callback numeric done/fail from publishing a second result. Existing
  authority-bearing Context completion after successful admission still works.
  Nonterminal command_error diagnostics are not completion. Once-only custody
  is scoped to this callback, not a new forever blacklist or global legacy claim.
- Fresh admission must precede opaque I/O. Snapshot the existing safe resource
  state at loan creation; a changed watermark revokes fresh admission without
  resetting it. Mark fresh custody unavailable at existing no-new-Io-ID outbound
  paths (plain/signed WS send, close and attaching deferred signing to existing
  work). This is a read-only watermark plus one scoped enum, not another resource
  allocator or physical evidence journal. Legacy I/O and its existing numeric
  completion may continue, but cannot later claim new pre-authority NotSent.
- Protect already-existing ownership independently of invalid clocks. NaN may
  make a deadline comparison false; it must never permit rejecting or deleting
  a prior generation. Only safely expired, unheld metadata may be retired before
  first issuance. Every typed refusal precedes owner creation; revalidate this
  invariant before rejection cleanup, which never retires unrelated requests.
- Leave the private loan present if the native callback unwinds; its presence is
  itself the sticky engine failure fence until host disposal. Clear on normal
  return before deferred commands or placement release. Supported Venue actions
  get the same engine loan; unsupported capability guards never receive one.
- Request Error classification belongs to the shared error core (claude-dazzy3).
  Coordinate the same kind/venue_error helper instead of introducing a competing
  venue or command-entry error table. The helper is additive; existing typed
  failure categories and transport/financial proof semantics stay unchanged.

Independent plan audit accepts one private scoped Io loan/state, no new registry,
resource counter or venue workaround. Source implementation follows these gates.

Source-audit refinements (before guard fixes): a non-finite command clock must
still treat any existing managed owner as live; failed existing-owner preflight
consumes entry custody rather than restoring legacy terminal rights. Generic
same-command Context issuance after automatic rejection or any terminal result
must refuse before reserving an identity, including legacy cached completion.
Sibling issuance while Bound and APIs outside the genuine callback keep their
existing behavior. Regression controls retain actual Engine callback delivery.

Additive companion prerequisite in this shared entry slice: expose
Cx::validate_read_root_context(&Context) -> Result<(), Error>. It composes the
existing actual1792 read-companion validator with exact equality to the registry's
completion_root. This grants no I/O, mutation, lifetime renewal or observer/group
authority. It lets the independently owned history bridge bind its constructor
and no-I/O terminal path to the original Read root rather than a same-owner child.
Keep existing expiry/completion semantics and full Context/issuer equality. Test
real issued root/child, foreign issuer, mutation sibling and post-terminal reuse;
the history owner consumes only a reviewed committed pin. No history.rs edits.


Entry-retention refinement, before implementation (owner approved 2026-10-23).

Problem: an admitted native WS read has no managed-HTTP pending/delivery count.
After its immutable deadline, unrelated Context issuance can prune its owner
before the native timer callback. The exact root then loses completion rights
and the caller receives no terminal result. Root provenance alone does not hold
its generation. Cached same-callback completion and managed attempt registration
are distinct cases.

Use the existing bounded CommandOwner registry as the sole authority. Add one
private `entry_owned` bit, false for generic http_context owners. On successful
`admit_http_command`, bind it synchronously to the exact newly issued original
root before returning; validate issuer/generation/root invariants privately.
No public retain API, synthetic Context, timer, deadline, counter or venue state.
Initial refusal never marks an owner retained and keeps existing typed cleanup.

All existing command-owner pruning/live/expired-owner checks include entry_owned:
- Registry issuance may expire Context metadata, but cannot evict the retained
  generation until its existing typed terminal path removes it.
- command_live and prepare_command_entry refuse duplicate/reissued initial
  ownership even after deadline. Generic same-command issuance after expiry
  refuses State instead of extending or replacing that root.
- Existing done_context/fail_context/command retirement removes the owner/hold.
  Read completion expiry tolerance remains separate from send/publication rights;
  no child, retry, HTTP publication or request deadline is renewed.
- The existing 256-owner bound still applies. If an adapter abandons an owned
  command without terminal cleanup, admission refuses Capacity; it never steals
  the outstanding caller's generation to reclaim space. Native WS timers keep
  their exact 5000ms deadline and remain responsible for native queue cleanup.
- Generic unowned http_context/background metadata keeps existing expiry cleanup.
  The later Plan phase can fold this predicate into its existing retained-owner
  phase; do not add a second retention registry or counter.

Commit plan, then real Engine assertion REDs before code:
1. Native read admitted at2/deadline5002, unrelated command admission at5003,
   then original native TimerFired: one Timeout terminal, no hang.
2. Same command ID repeated after expiry but before native timer: duplicate only,
   no native callback/timer/I/O; after old typed terminal, reuse gets fresh owner.
3. At256 retained owners after expiry, fresh command gets one typed Capacity
   failure; completing one exact generation frees one slot, unrelated owners
   remain. Generic expired same-command issuance cannot remint/extend root.
4. Preserve synchronous cached completion, original clocks/native timer IDs,
   unchanged send denial at/after deadline, all prior entry/typed expiry/integrity
   controls, normal native Binance behavior and legacy inventory byte-exact.

Verify failing controls first, implement only existing registry + entry-success
binding, re-run controls/full engine + focused native suites and strict/fmt/bans.
Independent source/inventory/preservation audit before publishing the ONE composed
#1830 pin (includes reviewed #1816/#1838). Caller docs distinguish retained
completion authority from live send permission; no claim of a new timeout owner.

Independent plan-audit refinements before implementation:

- The same private entry-owned bit also denies legacy numeric completion outside
  the initial callback. Only the issued root may retire this held generation;
  generic unowned Read handles retain their existing legacy compatibility. Pin a
  native timer numeric wrong-finalizer denial followed by typed-root completion.
- An entry-owned command retains its immutable original root bound. Generic
  unparented siblings may not extend that bound; issuance refuses an attempted
  extension or issuance at/after the original root deadline. This does not change
  generic unowned command behavior or renew send/publication rights. Test a longer
  sibling refusal before expiry, then expired same-command refusal, while the
  original native timer still finishes exactly once.
