# Managed socket core size design

Owner: codex-5c57jl. **Plan only; no implementation or build-policy change.**
Source inspection baseline: actual `c6a5c4bb0a406fccd93db00e9d9db270fce1112b`.
This supplements [the lifecycle contract](RUST_SOCKET_LIFECYCLE_CORE.md), rather
than replacing its authority, timing, retirement or financial obligations.
Bitget venue opt-s takes priority immediately after actual #2122 lands.

## Problem and evidence boundaries

55ao7t reports a local Bybit adoption prototype, `0ca830b2d28`, on #2127's
sorted-collection checkpoint: 1,396,154 -> 1,449,301 shipped bytes, +53,147.
The CI Bybit ceiling is 1,404,928; the reported local/CI offset is about 958 B.
That is roughly 43.4 KB over the ceiling, not a certified composition result.
Merely linking managed-sockets without adoption still costs about 47 KB after
that collection cut; adoption adds roughly 6.2 KB. Bybit already uses opt-z.
No threshold, profile, feature omission required by native behavior, or allowance
increase is proposed here.

**Attribution correction:** the earlier host-inlined +16 KB bucket and
ActionOut/managed-HTTP host growth were artifacts, not socket code. The earlier
normalizer stripped only sixteen-digit crate hashes; safe_cex_wasm's fifteen-
digit hash made unchanged host symbols appear removed and re-added. Supersede
that attribution and the initial coarse std/alloc and engine bucket figures.

55ao7t's corrected paired raw diff on actual #2127 landing 89a469d42b8 compares
Bybit without managed-sockets with feature-linked Bybit, **without adoption**.
It normalizes hashes with [0-9a-f]{8,16} and excludes the function-names section
(about +68.5 KB of names). Reported raw code+data growth is about 56.6 KB:

| Corrected raw bucket | Approximate growth |
| --- | ---: |
| engine::engine | 14.1 KB |
| socket_lifecycle | 11.0 KB |
| alloc::collections | 6.8 KB |
| socket_driver | 6.2 KB |
| alloc::vec | 6.1 KB |
| core::slice | 5.6 KB |
| core::iter | 1.7 KB |
| .rodata | 1.6 KB |
| socket::Supervisor | 0.7 KB |
| Bybit venue | 98 B |
| safe_cex_wasm host | **0 net** |

ActionOut::serialize is 4,005 raw bytes on both sides. WasmEngine's command,
new and hosted-signing functions are identical. These rounded buckets are
attribution, not additive shipped savings; remaining symbols and shared callers
also matter. Shipped bytes in that exact pair are **1,397,322 -> 1,444,400**,
**+47,078 B** for linking the feature alone. This is distinct from the earlier
adoption prototype and the c6a5 (a) experiment below; do not mix their absolutes.

The earlier unconfirmed five-row baseline table was superseded pending a fresh
pair. 55ao7t now reports the following **fresh paired** builds on exact c6a5
versus c6a5 plus (a)'s two inline-never attributes. All sizes are shipped bytes;
these are owner execution evidence, not builds performed for this document.

| Family composition | c6a5 baseline | c6a5 + (a) | Delta |
| --- | ---: | ---: | ---: |
| Bybit-ms, feature linked without adoption | 1,451,162 | 1,451,162 | 0 |
| OKX | 1,123,368 | 1,123,368 | 0 |
| Phemex | 1,139,332 | 1,139,332 | 0 |
| Bitget | 1,360,487 | 1,360,487 | 0; identical hash |
| Combined | 13,907,484 | 13,907,484 | 0 |

The four family baseline values are freshly reconfirmed; the earlier combined
13,908,609 remains superseded (1,125 B higher). Lever (a) is **NULL and dropped**:
LLVM already emits handle_managed_socket (1,490 raw B) and drain_socket_frames
(932 raw B) as standalone symbols. The reported raw difference is only ten
crate-hash metadata bytes. No PR or additional CPU run is justified for (a).
Do not subtract the earlier adoption prototype size from this feature-only
baseline; measure the prototype on the exact common composition/toolchain.

## Ownership and existing work

| Work | Owner and boundary | Evidence / relation |
| --- | --- | --- |
| #2127 sorted Vec map/set | 55ao7t; LANDED 89a469d42b8, tree 1d1261cb055 | Reported paired Bybit-ms -8,086 B, OKX -3,258, Phemex -8,079, Bitget 0, combined -11,350; not additional savings against a prototype already based on #2127. |
| (a) outline socket ingress/drain | 55ao7t; experiment completed and dropped, no PR | Fresh paired zero shipped bytes in all five compositions; not a remaining candidate. |
| (c) field-driven ActionOut encoder | Dropped as a socket lever by overseer | Corrected attribution shows zero host/encoder socket growth. General family shrink would be a separate assignment. |
| (d) shared replacement block | 55ao7t; #2140, fe7lgm CLEAR, run 40198956107 in progress when reported | replace_with_repair helper; no replacement semantic change or final CI claim here. |
| Unstable sort for unique interest entries | 55ao7t; approved measurement, local only | Map::from_entries stable sort adds roughly 5 KB raw slice code; caller duplicate refusal must remain ahead of construction. No shipped saving claimed yet. |
| S1 generic Cx/Io extraction | dazzy3, local d826748665e on refactor/rust-engine-plan-core-s1 | 101 P-free methods into CoreCx/CoreIo/CoreRef; dyn PendingDiscard for one removal. Only socket_epoch moves from engine_socket.rs. |
| Further socket structure | This design; implementation unassigned | Driver generic methods, Engine socket dispatch, continuations and storage are outside S1 and 55ao7t's retained levers. |

S1 on tip 76f1 reports combined -170,424 B, but single-family mean **+1,436 B**,
Bybit **+1,945 B**, Bitget -321 B. One Pending type already has one instantiation.
Erasure can reduce combined duplication without helping Bybit. It is not the
missing 44 KB and must not be credited as a per-family saving.

Current source already marks process_pending_socket_work, socket_refused,
socket_effects, take/restore_socket_driver, Driver::finish_ingress and
Driver::finish_control inline-never. Do not propose those
existing attributes as new cuts. Keep (d) and the unique-entry sort experiment
separate from this design; (a) is null and (c) is no longer a socket lever;
none of the estimates below may be added to their results without a union build.

## Proposed structure, in measurement order

### 1. Establish precise reachability before narrowing capabilities

Map every managed API call in prototype 0ca830b2d28, including all private and
public callbacks, reconnects, book-gap recovery and signed auth. The inspected
prototype uses register_socket_feed, connect_socket, socket_epoch,
set_socket_interests, retire_socket and ordinary send. In particular, **desired
proposals, normal batch release, replay and final signing barriers are used**;
none may be removed because there is no explicit venue call named release_batch.
Check the complete source/callback graph before declaring handoff, repair,
one-shot frames or snapshot completion unused. HTTP session waits are outside
this socket plan and cannot be disabled to fund its size.

Prefer one static capability declaration on P, shared by Cx<P> admission and
Engine<V> through V::Pending, following dazzy3's suggestion. A proposed
SocketCapabilities trait supplies FULL default constants (for example HANDOFF,
REPAIR and SNAPSHOT); implementations for audited venue Pending types may narrow
only proved-unused paths. Engine then omits the matching handoff/timer/drain
arms, and Cx admission specializes to the same declaration before allocation or
retaining a payload. No runtime flag or cached permission is needed. A separate
NONE declaration requires a proven non-adopter; Cargo feature unification alone
cannot select per-venue capabilities.

Cx<P> does not know V, so a Venue-only constant was insufficient. Pending-based
constants solve that boundary, but introduce explicit compatibility work:
Venue has one associated Pending type, not a globally enforced one-to-one
mapping. Shared unit Pending in probes/fixtures, or reused Pending across venue
variants, must retain FULL or have compatible declarations. Audit all consumers
before adding bounds; keep feature-off public Cx/Io users compiling. Trait
defaults do not provide a blanket implementation that can later be overridden
on stable Rust: enumerate explicit FULL implementations rather than relying on
specialization. Whether the bound lives only on socket APIs or on Venue's
associated type remains a separate API review decision.

Also separate plain desired proposals from optional handoff/repair work. A
misdeclared unsupported API call must fail with a reviewed typed refusal before
allocation/retained Pending, never silently ignore work or skip cleanup. FULL
consumers retain existing behavior. This declaration is a new API contract
requiring separate review, not an already approved refusal.

Default FULL means the feature-only non-adopter pays its present cost until it
explicitly proves NONE. Existing adopter behavior must remain identical for all
reachable inputs. Capability tests must deliberately drive each API and show
that no admitted work is stranded by an omitted dispatch arm. This reachability
work is the first implementation gate and belongs to this plan's owner;
dazzy3 stays off it unless handed a bounded slice. A whole Bybit-tree lexical
scan of prototype 0ca830b2d28 finds no call to set_socket_interests_then,
repair_socket_interests, send_after_socket_interests,
register_socket_feed_with_snapshot or complete_socket_snapshot. Handoff's sole
Cx constructor is set_socket_interests_then. This is a source-search fact, not a
complete callback/indirect reachability proof. First attribute the raw dominator
subtrees for that constructor and continuation dispatch/drain on the prototype;
use them to tighten the capability hypothesis before designing its API. Shared
callees and indirect roots must not be credited as exclusive savings.

### 2. Keep payload custody out of the shared continuation algorithm

Today Proposal<P> contains Option<Continuation<P>>; Continuation<P> embeds the
entire interest/revision/timer metadata next to P. Consequently plain desired
updates share collection/drop machinery with handoff Pending, even for consumers
that never request handoff. Split the owned metadata into non-generic proposal
and handoff records. Retain only the venue payload in a thin Pending table,
keyed by the existing checked, monotonic proposal ID; no second issuer, recycled
unchecked index, Any/downcast, unsafe cast or Box<dyn Pending> per record.

Move validation, byte/count accounting, revision rebase and timer eligibility
into the metadata core. The generic shell takes/drops the exact payload once
when the original path does. Preserve FIFO across proposals and effects, exact
retirement cancellation and no earlier destructor/callback. A plain proposal
must not link the full continuation admission routine solely because both
currently call propose_interests(..., Option<(required, P)>).

Compare a sorted bounded payload Vec against existing storage; do not assume a
new lookup container saves bytes. Keep a single canonical association between
metadata and payload and test it under removal, retirement and failed encoding.
Keep this metadata core in engine_socket/engine storage, outside
socket_lifecycle.rs::replace_with_repair; any overlap there waits for (d) to land.
Erasure is useful only if emitted code proves shared metadata/collection bodies;
otherwise retain the simpler original storage.

### 3. Consolidate collection instantiations without changing queue semantics

After #2127, inventory remaining VecDeque instantiations: Effect, QueuedBatch,
(Epoch, String), Proposal<P> and Continuation<P>. Raw alloc growth is evidence
for investigating element-specific insert/remove/drop code, not a mandate to
replace every queue. Prefer the metadata split above and a common non-generic
record where ownership permits. If a bounded Vec plus head cursor replaces a
queue, preserve logical FIFO, capacity, retained-byte accounting, drop timing,
remove/retain behavior and wrap/compaction behavior with trace parity controls.
No pop-front shift of a large queue in the steady path merely to save bytes.
set_socket_interests_then also retains a BTreeMap<&str, &Interest> and a
BTreeSet<&str> after #2127. Gating out handoff should remove these together;
otherwise compare sorted borrowed rows plus duplicate/required-key checks,
preserving original admission order and exact refusal behavior. Do not count
both that cut and its containing capability subtree. Do not duplicate #2127's
map/set work or 55ao7t's replacement helper (including its local unique set).

### 4. Narrow the Driver/Supervisor execution boundary

Driver is already a non-generic struct; its begin, timer, finish_control,
release_frame and retirement methods are generic only through Cx<P>.
Supervisor also takes Cx<P> for timers, jitter and action publication. After S1,
use S1's borrowed, non-generic CoreCx/CoreIo directly after S1 lands: raw now,
timer allocator/registry/action sink and existing entropy/jitter source. Do not
introduce a second scheduling wrapper. Driver decision bodies use CoreCx, while
venue/store/Pending/signing/native callbacks stay in the thin Engine<V> shell.
No new dyn Venue object or delayed command list is required.

This is an API sketch, not a second Supervisor or clock. Preserve direct,
synchronous calls: authentication_progress allocates the replacement timer
BEFORE clearing its old watch/origin; ordinary rearm keeps its own original
clear/allocate order; ID exhaustion retains the original fatal boundary.
A deferred vector of timer commands would change those semantics and is rejected.
Retirement cancels runtime work, revokes ownership, cancels signing/transports,
and completes native invalidation in the existing order before replacement.

S1 point B has CoreCx { now, io: CoreIo { core: &mut IoCore,
pending: &mut dyn PendingDiscard } }; Store exists there only with the
metadata-freshness feature. Read-only queries use CoreRef/CoreIoRef via
core_ref(). Keep Store-dependent decisions in the Engine<V> shell, independent
of that feature. Verify actual landed operations and visibility before choosing
signatures; do not move socket bodies before S1 lands and finishes regeneration.
Keep this work separate from generic engine
HTTP/Plan internals and from replace_with_repair. Combined may benefit strongly;
Bybit alone is likely neutral or slightly larger without reachability reduction.

## Per-family planning ranges (shipped bytes)

These are unmeasured hypothesis ranges, **not commitments or additive savings**.
A negative number means fewer bytes; positive means possible growth. Feature-off
Bitget is expected to have zero socket-only change, verified on every real pair.

| Candidate, applied after existing cuts | Bybit managed/adopted | OKX / Phemex | Bitget feature-off | Combined |
| --- | ---: | ---: | ---: | ---: |
| Proven unused optional socket capabilities | -8 to -18 KB if handoff/repair/snapshot are unreachable | 0 to -6 KB per family, only after their own reachability audit | 0 | 0 to -2 KB; FULL users keep most shared code |
| Metadata/payload and queue-instantiation consolidation, together | -1 to -5 KB | -1 to -5 KB | 0 | -3 to -12 KB |
| Driver/Supervisor non-generic scheduling boundary | -2 to +4 KB | -2 to +4 KB | 0 for socket-only code | -3 to -10 KB, excluding S1's existing savings |
| Explicit NONE non-adopter declaration | Up to the roughly 47 KB linked-feature overhead for a genuine non-adopter; **zero credit for adopted Bybit** | Not applicable to adopters | 0 | No promise; other adopters retain core |

Dazzy3's earlier rough -10 to -20 KB family hypothesis included (a)/(c)/(d)
and queue consolidation. It overlapped this table, preceded (a)'s zero-byte
result and relied on the now-corrected host attribution. It is not an active
combined savings estimate. Remaining (d), unique-entry sorting and storage cuts
need their own measured union. No host-encoder reduction is credited to sockets. These ranges do not establish
recovery of 44 KB. If paired union results remain short, report
the remaining bytes and code reachability before proposing another structural
change; Bybit stays parked and its allowance/CPU bounds remain unchanged.

## Behavior and verification gates before any source

Preserve two independent fresh message/epoch/deadline checks, equality expiry
before codec, current-epoch signer and final-send fences, first-pong activation,
Required/Observe/None auth, Progress without Ready, native refusal/retry policies,
snapshot/idle/ping bounds, invalid-clock handling and independent native watchdogs.
Preserve transactional coverage/sticky error/rollback, exact frame order and
cadence, group retirement before callbacks, callback-failure quarantine and fatal
resource behavior. Preserve handoff required interests, later ingress/timer
ownership, original HTTP/command delivery isolation and terminal projections.

For each implementation PR: review plan/API first, pin original semantic
controls, add structural parity tests for queue/capability/payload changes, then
source review. No test assertion, budget, CPU threshold or sample/tape changes.
Run scoped native/strict/feature-off checks with jobs4 sequentially; no full local
test sweeps. Generic engine work measures both Bybit and Bitget on the actual
composed tree; include OKX/Phemex/combined size as appropriate. CPU certification
uses CI only on this shared machine, one run per frozen head under current rules.

Measurement matrix: same source/toolchain/profile baseline versus one candidate,
then a measured union; each includes non-adopter feature-off, feature-linked
non-adopter and actual adoption where applicable. Copy raw artifacts immediately
before the next build overwrites them; attribute disjoint raw code+data, and use
optimized shipped web/node bytes as authority. Record exact commits/trees, flags,
Binaryen/bindgen versions, artifact hashes and same-run CPU baseline/bounds.
No arithmetic sum of independent PR savings and no net-size claim from twiggy.

## Plan checkpoint

No source, tests, Cargo, build scripts, allowances, inventory or CI changed here.
Coordination: dazzy3 confirmed the socket methods above are outside S1 and supplied
its adverse single-family evidence; 55ao7t confirmed ownership of (a)/(c)/(d)
and corrected the duplicated (a) measurement, then supplied the fresh null
result recorded above. Corrected post-#2127 raw attribution supersedes the old
host buckets: (c) is dropped as a socket lever. #2127 is landed; (d) and the
approved unique-entry unstable-sort measurement remain 55ao7t's work.
Dazzy3 CLEARed ec4b70f0316 as a
plan; 55ao7t reported no blockers, with baseline provenance, remaining handoff
collections and the (d) cut-line corrections incorporated here. Pending-based
capabilities and reuse of S1 CoreCx are design refinements awaiting exact API
review before source. Implementation order: actual prerequisites and levers,
reachability proof, one isolated structural candidate, measured union decision.
