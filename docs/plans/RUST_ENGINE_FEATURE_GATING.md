# Per-venue engine feature gating

Owner: claude-55ao7t. Reviewers: claude-fe7lgm (engine), claude-dazzy3.
Overseer priority (ddi2il, 2026-10-26): compile an engine feature only into the
`Engine<V>` of venues that use it. **Plan only; no source.**

## Problem

Cargo features are per crate, so enabling an engine feature for one venue links it
into every `Engine<V>` in that binary. The dispatch that reaches the feature is
generic and guarded only by runtime state (for example `IoCore::managed_sockets`),
which LLVM cannot prove false for a venue that never uses the feature.

Measured cases:

- **managed-sockets on Bitget**: +68,761 shipped per family (1,480,043 →
  1,548,804, web = node), with no adoption. Source: codex-ih35js's paired build on
  automatic tree 9b686e6fcd6 (tip 174f6a4c90c plus #2170), receipts under
  `/tmp/codex-ih35js-bitget-socket-plan/feature-link/`. On Bybit the same link
  cost +46,469 (12d7a19785a).
- **http-mutation-barriers**: +395K on the combined bundle when BloFin W2 enabled
  it (overseer report). Attribution is still to be measured; see Phase 2.

## Attribution: Bitget managed-sockets link, no adoption

Raw code and data of `raw.wasm`, with crate hashes normalized
(`[0-9a-f]{8,16}`) and the function-names section excluded: **+83,170 raw**.

| Bucket | Raw bytes |
| --- | ---: |
| `socket_lifecycle` (Lifecycle) | +20,367 |
| serde_json (`lexical` +7,446, `de` +6,931, other) | about +15,000 |
| `socket_driver` (Driver) | +12,475 |
| generic `Engine<BitgetFutures>` socket dispatch | +11,438 |
| `core::slice` | +4,975 |
| generic `Cx<Pending>` socket methods | +4,332 |
| `.rodata` | +3,608 |
| `alloc::vec` / `alloc::collections` | +2,503 / +2,275 |
| `socket::Supervisor` | +1,571 |
| legacy close/supervisor paths no longer reached | about −2,600 |

Top symbols include `Lifecycle::replace_with_repair` (2,953),
`Cx::socket_effects` (2,806), `Engine::process_pending_socket_work` (2,539),
`Driver::finish_control` (1,696), `Engine::handle_managed_socket` (1,477) and
`Engine::handle_socket_continuation` (1,194). The serde_json growth is the
float-lexical and value parser reached through managed interest canonicalization.

Every bucket is reachable only through managed dispatch that Bitget never enters.
Gating that dispatch statically should therefore recover nearly all of it; only
the `IoCore` socket fields' construction and drop glue remain. That residual is
small but has to be measured, not assumed.

## Mechanism: a per-venue `where Self: Sized` declaration, one per feature

The precedent already in the engine is `Venue::dispatch_nonce()`:

- it is a `where Self: Sized` default (`false`), so `Venue` stays dyn compatible;
- `Engine::new` copies it into `IoCore::nonce_enabled`;
- `Engine<V>` reads it statically.

#2161's `socket_capabilities()` used the same shape and measured −5,206 on Bybit
for gating handoff. An associated const was rejected there because it breaks the
pinned dyn compatibility of `Venue` (E0038).

**Phase 1, managed-sockets:**

- `fn managed_sockets() -> bool where Self: Sized { false }`, `#[inline]`. The
  declaration method itself is unconditional, compiled in every build, and only
  the dispatch it guards is behind the feature (fe7lgm). Adopter impls then need
  no cfg, and the trait's method set stays stable under feature unification, the
  same rule as for public enums.
- Every managed dispatch site in `Engine<V>` reads `V::managed_sockets()`, a
  const-folded branch:
  - the `handle_managed_socket` call in event dispatch;
  - the `drain_actions` socket block (`fence_socket_actions`,
    `drain_socket_frames`, continuations);
  - `process_socket_work`;
  - the managed timer arms.
- `Cx::register_socket_feed`, its snapshot form and #2177's native-timing form
  refuse `SocketError::NotPermitted` as their first statement unless the
  construction-time `IoCore` copy says the venue declared managed sockets. Work
  for a compiled-out path can never be admitted.
- The managed timer arms refuse to arm under `false`, not only skip dispatch, so
  no `TimerId` is allocated for a compiled-out path (fe7lgm).
- **Default polarity is `false`**, so existing and future non-adopters pay
  nothing without any edit. Each current adopter declares `true` in the same PR:
  OKX, Phemex, Hyperliquid, TxFlow, Backpack, WEEX and WOO, plus Bybit (#2180)
  and Kraken when they land. A missed declaration fails closed at registration
  and is caught by that venue's own managed-socket tests. It never silently
  degrades. Each adopter crate also gets a one-line test asserting
  `V::managed_sockets()`, so a dropped declaration fails that suite directly
  even where its managed-socket tests are sparse (fe7lgm).
- `managed_sockets()` and #2161's `socket_capabilities()` compose: capabilities
  narrow inside an adopter, and `managed_sockets()` decides whether the venue
  adopts at all.
- **Sites that cannot fold on `V`** (dazzy3 review). Code inside
  `impl<P> Cx<'_, P>` cannot see `V`, so its checks stay runtime. Inventory and
  measure each one:
  - `managed_send_allowed`, reached from `Cx::send`, `send_signed` and possibly
    `close`. It reads `io.core.managed_sockets`, then the Lifecycle epoch query,
    which keeps some Lifecycle state-query code linked for legacy venues.
  - `Cx` socket helpers reached from engine internals rather than the venue
    (for example `socket_effects`). Fold them at their `Engine<V>` caller.
  - `Engine::sign_failed`'s WsSend/WsConnect arm (`managed_socket_active`,
    `socket_epoch`, `stop_socket_feed`). This one is `Engine<V>` and folds;
    list it with the gated sites.
  - The construction and drop glue of `IoCore`'s `socket_driver`,
    `socket_effects` and `socket_after`: the expected residual.

  Rule: copy the declaration into an `IoCore` bool at `Engine::new` (as
  `nonce_enabled`) only for the registration refusal and true leaf checks. Move
  every dispatch decision to an `Engine<V>` caller wherever one exists. The
  prototype pair measures what is left. If the `Cx<P>` residual proves material,
  a static path exists that keeps `Venue` dyn compatible: production `Cx<'_, P>`
  instances are effectively per-venue, so a hidden sealed trait on `P` with an
  associated const (impl'd for adopter Pending types, defaulting via a blanket
  marker) can fold those checks too (fe7lgm). Measure first.

**Phase 2, http-mutation-barriers**, then any later per-venue feature: the same
shape, `fn http_mutation_barriers() -> bool`. It comes after a measured
attribution of the +395K and an inventory of barrier entry points (group release,
`prepare_group_http_release`, the planned-mutation paths in `http_lifecycle` and
`request_context_planned`). Attribution comes first because the +395K is a
combined figure, not a per-family one. Much of it may be Plan-core duplication
across adopters rather than barrier code, and gating cannot recover that
(fe7lgm).

**Why gating beats erasure for barriers** (dazzy3 review): erasure keeps one
shared copy plus a generic shell per adopter, and S1 showed it saves combined
duplication but nothing per family. With a single barrier adopter, gating
removes nearly all of it from the other `Engine<V>`. Erasure is worth a look
only later, when several adopters make the shared copy dominant. Other engine
features (dazzy3's write-preflight-wait and release-domains) can take the same
declaration shape.

**Alternatives considered:**

- Type erasure (S1 style) reduces combined duplication but not single-family
  cost: S1 measured +1,945 on Bybit.
- Splitting the generic engine into adopter and non-adopter types is a large
  refactor of every `Engine<V>` caller, with the same per-feature knowledge
  required anyway.
- A declaration defaulting to `true` would force edits on about 25 non-adopters
  and protect no future venue by default.

## Phase 2 measured: http-mutation-barriers

Measured on BloFin W2's exact composition (closed #2179, head 1a176871f8b; OFF tree
174f6a4c90c), combined shipped bytes: barriers OFF 13,994,169; W2 ON 14,381,721
(+387,552); W2 plus the gate 14,058,690 (+64,521). The gate recovers 323,031 (83%).

The ON/OFF raw delta (+455,924) was almost entirely `Engine<V>` group machinery
monomorphized into all 29 venues: `claim_http_group`, `prepare_mutation_group`,
`mutation_group_members`, `announce_mutation_group`, `prepare_group_commit`,
`current_group` and about 50K of per-venue `sort_unstable_by_key`. The gated
residual (+69.9K raw) is 25.4K non-generic barrier core, 19.1K BloFin's own code,
and barrier branches inside common generic functions, which can take an inline
`V::` check later.

Mechanism, the same shape as Phase 1:

- `Venue::http_mutation_barriers()` is an unconditional `where Self: Sized`
  default (`false`).
- `Engine<V>` returns early at the three group entry points:
  `pump_mutation_group`, `http_group_pending` (false) and `claim_http_group`
  (`Error::Invalid`).
- Group admission refuses first, in the shared `admit_plan_owned`, through an
  `IoCore` copy written only by `Engine::new`. An undeclared venue therefore
  cannot admit a group its compiled-out pump would never drive, and spends no
  ID, action or plan context; this is pinned by a control.

Ruling (ddi2il): the engine gate lands on its own, and no venue enables barriers.
BloFin W2 stays closed: +64.5K combined to restore parallel hedged leverage, a
rare operation, isn't worth it, and the serial join is safer. Any future barrier
adopter declares `http_mutation_barriers()` with a declaration test in its own PR.

## Behaviour and verification gates

- Adopters keep identical behaviour: the declaration folds to `true` and every
  branch is today's code. Their full suites and the feature-mode matrix rows run.
- Non-adopters: a managed registration attempt is refused before any activity or
  allocation, pinned by a control. With no attempt, behaviour is unchanged.
- `Venue` stays dyn compatible, pinned by the existing
  `default_nonce_capability_preserves_venue_trait_object_compatibility`.
- Size, measured, never summed:
  1. one prototype pair on Bitget's feature-linked composition, before source
     review (the existing enabled build against enabled plus the gate);
  2. a feature-OFF zero-growth check from CI's family records;
  3. one local combined pair on the composed tip.

  Bitget's managed-socket adoption stays on hold until the measured residual is
  known.

## Order

1. This plan: fe7lgm and dazzy3 review.
2. Originals: a non-adopter refusal control that spends nothing (no FeedId, ID,
   timer or allocation), per-adopter declaration tests, and adopter parity
   unchanged.
3. The Phase 1 prototype size pair.
4. Phase 1 source PR, one CI run, auditor GO.
5. Phase 2 attribution, then its plan delta.
