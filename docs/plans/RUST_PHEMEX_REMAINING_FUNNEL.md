# Phemex remaining request funnel

Owner: codex-ih35js. Engine reviewer: claude-fe7lgm or claude-dazzy3.
Original source base: 7ada358c4460ec1e0965f776f5494c628efb3cbd.
Status: supported-path source implemented locally after original-control review.
Source review, paired size measurement and full CI remain gates. Scope excludes
commandless late repair under the overseer ruling.

## Scope and preservation

The checker was re-dumped on this base: 26 production fingerprints / 28
occurrences, exactly matching its manifest. Retire the remaining history,
income, credentialed candles, discovery, depth and trading funnel. Preserve
landed #1974 dispatch recipes and #2070 identity/account/reconcile ownership,
unsigned PublicTrades and public candles, managed sockets and all native wire,
financial reducer and reconciliation assertions. No generic engine edit is
planned in the consumer. No new legacy fingerprints or signing primitives.

## Authority and selection

- Private history and income admit one genuine caller-receipt Read root before
  native active-set insertion or rate-limit deferral. Use
  SelectedRead::command_root, ReadRoot::leg and ReadLeg::queue; the immutable
  selection owns every physical child. Legs are 20s/Never, minted only when
  eligible. Retire each old leg before another page or currency stage.
- Fills/orders use ReadHistoryRows. Income uses ReadRangeHistory and its actual
  RangeWalk, PAGE200, seven-day windows, USDT then USDC, native accepted-output
  recovery, duplicate occurrence IDs, sorting, 100000-result cap and truncation.
  Lookup uses one Read root and native found/definitive output, never an invented
  absence on malformed, provider or transport failure.
- Data validates exact root+leg before count, raw row parsing, staging or cursor
  advance. ReadCompletion::from_context binds the same root, without a second
  terminal. Root retirement drops exact Owner work and native timers become
  inert; no local root timer, renewal, finalizer or INCOMPLETE on root expiry.
- Native local validation order and messages stay in place. Query IDs, duplicate
  rejection and 16-owner caps never clear a newer owner with the same ID.
- Ordinary private GETs prefer Secondary then Primary. Client-order lookup stays
  Primary via ReadRole::Primary(ClientOrderLookup), preserving its Primary route.
  Every mutation is Primary/Mutation. Declare no new account-read evidence tag.
- Optional-key public endpoints retain their native signed selection: products,
  ticker, depth, credentialed candles and credentialed PublicTrades sign with
  Secondary else Primary, even Secondary-only and public_data_only configs.
  They go unsigned only when native did. Existing private read_slots requires a
  Primary; do not silently transplant that policy to public reads. Use shared
  Slots with explicit native per-caller policy and pass selected Identity into
  the existing Http recipe. Build both public/private Slots together in the
  existing read_slots setup from the same validated identities; no new read-key
  extraction path. Preserve read-key validation/startup precedence.

## Discovery, depth and candles

Discovery and depth use Background Read contexts, timeout20s/Never. Optional
signing uses SelectedRead::background with the public-read selection policy;
keyless reads use http_context(Background, Read) and request_managed. There is
one actual managed send per path, no eager sign_http fallback. Complete/retire
catalogue before account/ticker followups, ticker before public socket startup.
Exact delivery gates precede native_symbols, catalogue_partial, rows, Store or
publication. Failure-only cleanup retires exact context before native recovery.

Preserve discovery backoff, partial catalogue filtering and request bytes.
Depth keeps generation, 2000-update SyncBuffer, noncontiguous increasing sequence
policy, native20s deadline and1s resync eligibility. Consume only a current
snapshot; removed/readded subscriptions and late DATA cannot affect the new
book. Physical expiry and native timer equality must recover once, either order.
Credentialed candle roots are Read commands; keep rate-limit refusal before
unsupported interval, exact resolution/from/to rounding, row validation and
ascending timestamps. The existing unsigned branch remains unsigned.

Native429 evidence is a separate rate-limit concern. Pin the current prelude
and expiry policy before moving guards; retain the existing PublicTrades stale
429 suppression. Other stale responses get no DATA or publication rights.

## Command-owned writes

Admit begin_preflight(WriteAfterReads, read credential None, Primary write) at
the genuine entry;
validate the complete deterministic native job before any optimistic Store,
progress, account revision or dispatch effect. Preserve native precedence:
planning is computed first but duplicate/jobs32/credentials/settings gates win
before its error is published. Move acknowledged/side/bracket pruning behind
admission; do not mutate them for an unadmitted command.

Use admit_serial_plan with SerialCall{Mutation, timeout_ms:20000, attempts:1}
and SerialPolicy{Primary credential, pacing:None, refusals_complete:false}.
No root wall-clock or added step limit. Preserve native per-input safe-split
checks and total 1000-order job validation, including multi-input jobs and
sequential cancel/replacement/bracket ordering.
Empty keyed jobs use done_empty_plan, no dummy call. Issue the available slot
through plan_leg_context and request_signed_managed with the unchanged recipe.

Each DATA callback validates_plan_delivery before decoding or Store/progress.
Classify from native envelope/row evidence, record http_write_fact or
http_write_batch including ordered accepted IDs, then continue_serial_plan
only from exact proven DATA. No write resend: attempts1 and Never. Native
placement rejection stops. Cancel row errors, including mixed accepted/refused
rows, currently stop as Unknown: retain confirmed Store effects, restore only
unchanged refused rows, preserve native IDs and reconcile without resend. Pin
that native policy; do not transplant KnownPartial continuation from another
venue. Record PartialUnknown for such a native unproven failure. If a genuinely
definitive mixed fact is later supported, record KnownPartial, finish_plan_call
and fail_plan to stop with IDs preserved, never continue by accident. Other
ambiguous/malformed/HTTP/transport failures also stop and reconcile without retry.
Preserve client-ID progress after admission but before physical release.
Terminal projection uses the shared journal via done_plan/fail_plan; do not
bypass it with numeric done/fail or fabricate accepted IDs.

Actual signing release remains the sole source of signed_until. Unknown
reconciliation retains the observed uncapped raw signing expiry plus5000ms,
never now+inventedTTL. Preserve request/revision ownership, account refresh,
optimistic restore guards and replacement revalidation. Typed provider/local
refusals are NotSent before release; later failure preserves earlier evidence.

## Shared-core gap: commandless late repair

Native cancel_late/late_next sends cancel-by-order-ID with Request.command=None
from a later private stream or reconciliation read, after original commands
have finished. It sequences one repair write at a time, emits diagnostics and
refreshes/reconciles; it never sends a command terminal. This behavior is pinned
by the unchanged native late_landing and two-group cancel controls.

At this base there is no reviewed constructor for this commandless Mutation,
confirmed by fe7lgm. Background+Mutation is refused by design. Read executions
cannot become writes; planned executions require a genuine founder Preflight.
Session authority is for authentication, not order repair. Do not fabricate a
command, reuse retired authority, copy a venue issuer or relabel this as Read.

The overseer ruled: migrate supported paths now and keep cancel_late/late_next
as a named, unchanged exclusion. Preserve the existing legacy repair dispatcher
and its common request helper; no new legacy site or alternative managed bypass.
The intended retained live fingerprints are listed below; re-dump before freeze
and retain only the exact still-live entries, with no count growth. JSON cannot
hold comments: this plan and CALLERS name the prerequisite, while the ratchet
records the holdouts. The initial consumer is not a 28-to-zero claim.

| Repair holdout file | Existing fingerprint | Count |
| --- | --- | ---: |
| venues/phemex/src/lib.rs | fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6 | 1 |
| venues/phemex/src/lib.rs | 34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc | 1 |
| venues/phemex/src/lib.rs | 6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219 | 1 |
| venues/phemex/src/trading.rs | e6f9b79a027f31adbd5a2b4c25669770848c36a0b761835a87f5783a961f4790 | 1 |

The named prerequisite is **Constrained known-order cancel repair**. After this
consumer's source is up, codex-ih35js writes its separate core plan, fe7lgm
reviews it, and a second real venue consumer must be named before approval.
Phemex's remaining repair fingerprints go to zero on that actual core landing.
The shared primitive requires its own core plan/review: constrained cancel of
an already-known order, no place/amend or command terminal, uncertain-write
fence/order-lock composition, Never, managed signing/expiry/pacing, and a second
real venue consumer. This consumer does not implement that shared policy.

## Original and source-stage controls

Originals must compile against unchanged runtime and execute genuine assertions,
with no ignored REDs or fabricated managed fallback:

1. Managed Prepare before release for products, ticker, depth, credentialed
   candles, fills, client lookup, income and writes; private/Public signed
   selection and exact native request bytes are positive controls.
2. Unclaimed products/history cannot publish rows, symbols, Store or terminals.
3. Private Read entry/deferred caller expiry, stale ID reuse and peer survival;
   native16-cap/duplicate/range and local precedence remain positive controls.
4. A genuine exhausted managed registry cannot publish optimistic order/status
   or progress for a rejected trade, and all existing peers remain live.
5. Native stream-triggered late cancel after Unknown+cancel-all remains a real
   cancel-by-ID exactly once, no new command terminal; add managed control only
   once its reviewed constructor exists.

Source-stage gates (not claimed as originals): delayed provider/route/Claim,
typed first-page failure vs later accepted-prefix recovery, exact physical/root equality
orders, provider refusal, zero eligible/accepted income prefix recovery, both
currencies, staged malformed rollback, stale429 policy, depth unsubscribe/
readd, native retry/resync timing, max1000 split+brackets and multi-input order,
mixed/refused/unknown writes, prior accepted IDs and id-less effects, released
signing-expiry reconciliation, concurrent settings/duplicate IDs and late repair.
Retain every existing assertion; fixture glue may drive genuine host handshake.
The unchanged trading::late_landing_seen_on_the_private_stream_is_canceled_immediately
control was independently executed GREEN. A duplicate setup in the new target
was discarded because it did not reproduce that fixture; no runtime diagnosis
or clearance is inferred from that discarded attempt.

## Inventory, measurement and freeze

At the first source commit measure Phemex and combined before/after on the same
composed tree with unchanged build profiles/toolchain. Add Bybit and Bitget if
any generic engine source changes. Keep raw/shipped artifacts and hashes outside
Git. Do not infer CI bytes from local offsets, add isolated savings, or raise a
budget without the overseer. Record the exact prune by file/fingerprint; signing
and credential inventories must not grow. Recheck actual-tip bans/ratchets.

Require fe7lgm or dazzy3 exact source/custody review, scoped Phemex tests and
strict Clippy with CARGO_BUILD_JOBS=4, formatting and existing ratchets. After
review corrections prepare one intended freeze head, push a draft once and
use one PR-triggered Rust run. READY requires green CI and source CLEAR; landing
requires 8x5tm1's exact-tree GO. Bitget size cuts remain separate after #2122.

## Original inventory

| File | Fingerprint | Count |
| --- | --- | ---: |
| `venues/phemex/src/history.rs` | `007e1c20fd9ad3fe2cb38a2646b5f0910bcced3498841d2ec9362b0da3b4e4f8` | 1 |
| `venues/phemex/src/history.rs` | `aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f` | 1 |
| `venues/phemex/src/history.rs` | `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 2 |
| `venues/phemex/src/history.rs` | `a25ed19569db55f160c6e07637c3dfe1a7151e2722da2780fb332e3fe3cab917` | 1 |
| `venues/phemex/src/history.rs` | `139c5990615d19868461cf9c3fc7c97d895017fced401b219198883e89eeefd1` | 2 |
| `venues/phemex/src/income.rs` | `891103f78a8eca3e3afa6df1d04856f35db60eaddb365f9e92293271fa46c893` | 1 |
| `venues/phemex/src/income.rs` | `f208d33f03a5291e0ba68dd6417e98dd3fd08398627717ab367d630192c297c2` | 1 |
| `venues/phemex/src/lib.rs` | `5993f99450d63ae5eb78cf2899ffc8efaaa9fd47dd066fcd1ac85a4cb8d937eb` | 1 |
| `venues/phemex/src/lib.rs` | `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 |
| `venues/phemex/src/lib.rs` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 |
| `venues/phemex/src/lib.rs` | `6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219` | 1 |
| `venues/phemex/src/lib.rs` | `4dbce16a94437affd078f0756fdc5bf35899434ebde3ba73925201568fb05469` | 1 |
| `venues/phemex/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |
| `venues/phemex/src/lib.rs` | `3d411e9715932bdc90a7a565f468fcb4804f1d8ef3c91c122626f77b2d90df2b` | 1 |
| `venues/phemex/src/lib.rs` | `415cde8bbb79aec0ffd251892d668f5fea56c30150ff07e0a3a1e8d1ebb85a04` | 1 |
| `venues/phemex/src/lib.rs` | `3707a3e4c331d8aa244ffa1fdfff00869707d624a6ed97afed2f783400c15f54` | 1 |
| `venues/phemex/src/lib.rs` | `1d1591136af8d3c7b891a725138b1fe1e9ef3e5d401e18f610b77d9866b877e3` | 1 |
| `venues/phemex/src/lib.rs` | `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 |
| `venues/phemex/src/lib.rs` | `1b0b8cbc9ce44a55975ea8f3e025691cecfa45581828e95eb50ccc2f2d18e4f1` | 1 |
| `venues/phemex/src/lib.rs` | `6d16631ed954d85c787eae05eab608a09c8ce735e57abce7ab9ccfe2f305d968` | 1 |
| `venues/phemex/src/lib.rs` | `60f3aecbbb9e98af4226ed2769775e3b26ba4af804afa0def383aad527bddd1b` | 1 |
| `venues/phemex/src/lib.rs` | `7b15b10003401f5b84eb1992913534cdc7868ab658ed9210e32d7230da28218c` | 1 |
| `venues/phemex/src/trading.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 |
| `venues/phemex/src/trading.rs` | `db4da6f92276f049047ceb4f85c06083f4c683531dd3d76b0a5ba44d9e2d5cb7` | 1 |
| `venues/phemex/src/trading.rs` | `e6f9b79a027f31adbd5a2b4c25669770848c36a0b761835a87f5783a961f4790` | 1 |
| `venues/phemex/src/trading.rs` | `06d49adf719c2e5ba5712d013a8bd485f49a8559eab9fbd74b8265789e9e3c9c` | 1 |

## Original checkpoint execution

On unchanged runtime at7ada358c446, the dedicated target executes22 controls:
13 intended assertion REDs (actual eager release, early DATA publication,
caller deadline omission and optimistic Store under genuine capacity pressure),
9 native GREENs, zero ignored. Native positives pin optional Secondary-only
public signing, read-vs-lookup selection, deduplicated fill values, exact order
bytes/progress, actual signing-expiry reconciliation, empty-job credential
precedence, 1000 split calls with ordered IDs, depth buffering and mixed cancel
Unknown/row restoration. The separately executed unchanged native late-repair
control passes. Target source contains no fake managed constructor or numeric
fallback. Strict test-target Clippy (-D warnings) and scoped formatting pass.

## First source checkpoint

The supported paths now use the reviewed SelectedRead roots/legs and serial
Mutation Plan. The request manifest drops 28 occurrences to the four named
repair holdouts above, with no new sender. Public optional-key selection uses
its own policy over the same trusted identities; private account/read selection
and the unsigned PublicTrades sender remain as landed. No generic engine,
provider recipe or host source changes are included.

Released 408/5xx responses are sealed Unknown by the engine without venue DATA.
The exact owner-retirement hook therefore retains only the live native write
for deferred settlement. Before the next Cx callback processes its input, the
venue restores native optimistic rows and records the raw signing-expiry plus
5000ms reconciliation state. On command entry, Store restoration and state
recording precede planning, while account-refresh/reconcile I/O follows genuine
command admission: allocating recovery IDs before begin_preflight would revoke
its fresh entry. The existing account helper is split into state recording and
its unchanged scheduling wrapper. Retirement creates no terminal, retry or
response authority. Successful/failed native reducers remove their jobs before
retirement, so settlement cannot repeat. Empty cancel-all still warns and
records its late-cancel obligation before done_empty_plan, after the genuine
credential-checked Preflight, with no dummy call.

The 20s child bound now includes route preparation, provider work and signing
queue time. History/income retain the original caller receipt; serial writes
have no whole-job timer or step-count cap. Every later income-page failure,
including integrity, signing/provider, local submission and transport failure,
returns only the previously accepted rows with the native INCOMPLETE marker.
The failed page's staged rows are discarded. A failure before any accepted
output remains typed, with no rows and no marker. Original-root expiry still
retires its exact owner; a stale failure cannot manufacture a completion.
Original history transport fixtures now supply Network explicitly, rather than
using a generic Internal error to represent a connection loss.

Native fixture changes drive real Prepare/Bind/Claim transitions. The immutable
selection is asserted at Prepare.intent; the released Primary packet has no
route override. Timer fixtures select the still-live native timer after managed
HTTP timer cancellation. Existing wire, numerical, order-status, reconciliation
and terminal assertions remain; trading.rs tests are byte-exact to the base.
Probe wrappers forward the landed release/reconciliation/retirement hooks.

Scoped execution: all 313 Phemex tests pass, zero ignored, including all 65
unchanged trading tests. The dedicated target now has 27 passing controls:
its 22 originals plus typed first-page integrity, caller-root equality
and reuse, physical timeout, and released 408/500 followed immediately by
cancel-all. The last control pins no InvalidInput, one original Unknown terminal,
no resend, restored Store rows and the exact signing-expiry reconciliation fence.
Strict all-target Phemex Clippy (-D warnings) passes. The local request dump is
exactly the four holdouts, with no signing/credential growth. Full bans passes after plain-merging actual tip c6a5c4bb0a4; the stale foreign
Backpack row is gone. Exact source review and full CI remain separate gates;
no READY is claimed.

## First-source size pair and composition gate

Frozen a634e821894 -> 7f6c35a3086, Rust1.99.0/Binaryen132, unchanged shipped
web/node profiles and jobs4:

| Bundle | Before bytes | After bytes | Delta |
| --- | ---: | ---: | ---: |
| Phemex | 1,139,332 | 1,231,469 | +92,137 |
| Combined | 13,908,332 | 13,959,753 | +51,421 |

Both existing budgets pass. The overseer accepted both deltas; combined has
196,023 bytes remaining at this pair. Raw combined is 31,545,802 -> 31,653,471.
The PR records the top ten positive raw Twiggy symbol deltas, normalizing Rust
crate-disambiguator hashes. Renamed/extracted venue functions are paired with
the removed originals, so their full bodies are not claimed as new shipped
cost. No cheap behavior-preserving 5KB cut was identified: the largest genuine
new item is the shared Plan admission instantiation, whose ownership checks
cannot be discarded. No generic engine optimization is folded into this venue
cutover. Re-measure combined on the actual composed landing tree after Bitget
slice3; this first-source pair does not waive that landing gate.

## Review correction and native prefix policy

The unsupported trading variants re-enter the common planned-Err gate chain:
duplicate/jobs32/private-key/settings precedence, InvalidInput for the final
unsupported refusal, NotSent and the native command_error diagnostic. New
controls pin public_data_only and 32 live admitted jobs. The latter are genuine
queued Plans, rather than manufactured Contexts.

The overseer requires native accepted-prefix income recovery for every later
failure. Before changing source, the corrupt-USDC-page and real delegated
signer-refusal controls both failed at expected prefix-success assertions on
the first source. They retain accepted USDT rows plus the native Warn-level
INCOMPLETE marker before completion; a first-page signer refusal remains
InvalidInput/NotSent without a marker. The separate strict-prefix rejection and
failure-kind dispatch helpers are removed, rather than retained as another
policy route. Read ownership, exact leg retirement and failed-page rollback
remain shared and unchanged.

Other review corrections preserve HTTP error bodies, code-less error-envelope
messages and native missing-list wording. Contract errors during Plan issue,
fact recording or settlement clean up the exact job and attempt its Plan
terminal with a diagnostic. An unissued next leg preserves earlier accepted IDs
and refreshes/reconciles without inventing a lost order. Retirement fences only
when its current native write actually released, retaining the job's maximum
observed expiry then. Controls cover non-command settlement, restored Canceling
rows and a retired middle split. Every lookup route proof now ties Primary
Prepare.intent to the actual by-order-id request; the pressure wrapper forwards
Read-root retirement and missing-list callbacks.

The correction adds public-only/32-job native precedence, non-command/cancel/
mid-split retirement, native failure text and a real delegated provider refusal
before versus after accepted output. The unsupported-command controls also fail
when the reviewed early-return regression is restored. Only native income
recovery expectations replace the rejected strict-prefix expectation; no old
financial assertion is weakened. After cleaning only this lane target, fresh execution passes all 320 scoped
Phemex tests (34 in the remaining-funnel target), strict all-target Clippy and
formatting. Both prefix REDs reproduce against the original income/lib source;
both precedence controls reproduce against the old early-return mutation. Full
bans/ratchets pass on the restored correction source.
