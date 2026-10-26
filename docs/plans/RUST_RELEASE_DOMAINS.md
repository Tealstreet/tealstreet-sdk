# Venue release domains: FIFO, release-paced REST (`release-domains`)

The feature widens and renames the earlier `serial-pacing` draft:
`release-domains = ["serial-plans"]`, so it keeps the serial-plans dependency.
It is non-default.

Owner: claude-dazzy3 (overseer assignment, widened to Coinbase).
Reviewers: codex-uufub4 (Plan core, nonce and release; owner of the Coinbase
remaining-funnel target) and codex-zs5e83 (Hyperliquid consumer; owns HL, GRVT
and the KuCoin adoption). Overseer ruling: release-anchored 334 ms, with one
all-REST FIFO domain for Coinbase.
Base: `feat/rust-rewrite` `b0ee0a9a4d5`. **Plan only; no source.**
Consumers:
- Hyperliquid trading (`RUST_HYPERLIQUID_TRADING_CONTEXT.md`, the
  "Actual-release pacing" gate);
- Coinbase REST (`venues/coinbase/src/rest.rs`).

This supersedes the serial-only draft at `9ab460c3a4e`, which only let a serial
Plan name a registered floor.

## Native behaviour to reproduce

| | Coinbase (`rest.rs`) | Hyperliquid (`trading.rs:582/701`) |
| --- | --- | --- |
| Members | every REST request: public and private, reads, writes, discovery | trading writes only |
| Order | one FIFO, in `request()` call order | one job at a time |
| Gap | 334 ms, measured from the previous dispatch, which is also its send and sign | 334 ms, measured from the previous write's queue time |
| Bound | queue of 256 | none |

**Ruled anchor:** the gap runs from the previous member's final Claim release,
reusing the registered `SendPacing` floor that `commit_claim` advances. This is
an explicit, overseer-approved behaviour change for **both** venues, not native
parity:
- Coinbase anchors at its own dispatch, before any managed Prepare, sign or
  Bind phase.
- Hyperliquid anchors at queue time.

Both are never faster than native, and both may add latency under delayed host
phases. Example: queue 0, Claim 1000, DATA 1001 sends the next request at 1334
instead of 1001. CALLERS records this for each venue.

There is no venue timer, no shadow floor and no queue-time ledger.

## Design

### 1. Declaration (one authority)

The venue declares, `Self: Sized`, default none, the same shape as
`socket_capabilities` / `dispatch_nonce`:

```rust
// Default `Self: Sized` method, so `Venue` stays dyn compatible (as socket_capabilities).
fn http_release_domain() -> Option<ReleaseDomain> where Self: Sized { None }
pub struct ReleaseDomain { pub pacing: Pacing, pub members: DomainMembers }
pub enum DomainMembers { AllRest, Mutations }
```

- `pacing.group` must be a group the venue registers in `http_pacing()`.
  Engine construction refuses a domain whose group isn't registered, or whose
  literal gap differs from the registered one.
- The effective domain pacing is stamped **before** any barrier or serial
  validation (uufub4 P2-3):
  - `validate_mutation_group` / `validate_zero` still reject a positive-gap
    atomic group, including a caller Policy with `pacing: None` inside a domain
    venue;
  - no barrier is split, and there are no simultaneous paced barrier releases;
  - the dispatch-nonce barrier refusal is kept.
- The **engine stamps** the domain's pacing onto every member Context it issues
  for this venue. That covers command, background, discovery, Preflight call,
  Plan leg and serial call Contexts. A member's own `Policy.pacing` must be
  `None` or that same group, otherwise `Invalid`, so no Policy can bypass the
  domain or join a second one.
- Coinbase declares `AllRest`; Hyperliquid declares `Mutations`.

### 2. FIFO, enforced at Bind

- **The one linearization point** is the engine accepting a managed request:
  the successful `request_managed` / `request_signed_managed` call that
  registers the physical attempt. Plan and serial calls register at that same
  point.
  - Context creation, `plan_leg_context` and call reservation do NOT enter the
    queue, so a leg that is never queued cannot block later native-order work.
  - A Stale re-prepare keeps the same attempt and its FIFO position, with a fresh
    preparation and permit. Its deadline, authority and repair budget are
    unchanged.
  - A retry after release is a new physical issuance at the tail; it never
    reclaims the old slot.
  - There are no ghost heads: a member leaves the index on every terminal.
- **Hold (uufub4 (c)):**
  - It shares packet storage and lifecycle plumbing only, not mutation-group
    membership or atomic release.
  - It has its own dedicated domain hold and index.
  - Domain and bearer gates are conjunctive: Bind waits for both.
  - Each held packet has a single owner and exact cleanup.
  - No locks or callbacks are held across commits.
- A member that finishes preparation **holds its packet and permit** instead of
  emitting `Bind`, using the same seam that barrier group members
  (`pump_mutation_group`) and parked bearers use. It emits `Bind` only once it is
  the domain head.
- The head advances when the current head's attempt is released, or reaches any
  terminal before release: a preparation or signing refusal, cancellation,
  retirement, or deadline.
- A head that is still preparing blocks the queue, as Coinbase's single queue
  does. A signature gone stale while held hits the existing Stale/re-prepare
  repair at Claim; a re-prepare keeps the member's place.

### 3. Gap, enforced at Claim (unchanged machinery)

- The head's Claim runs the existing `prepare_claim` pacing check. That gives
  `WaitUntil(prev_release + 334)`, or `TimedOut` if the floor is at or past the
  call deadline, i.e. NotSent before release.
- `commit_claim` alone advances the floor. A refused, stale or duplicate claim,
  or a `WaitUntil`, spends nothing.
- **Nonce (uufub4 P2-2):**
  - `reserve_nonce` runs during recipe preparation, so a held or `WaitUntil`
    member already has its initial reservation.
  - A `WaitUntil` adds no new reservation, and commits no released-nonce floor
    and no pacing floor.
  - A Stale re-prepare CAN reserve a new nonce.
  - Reserved maxima are never rolled back, and the cumulative 8-repair budget is
    never reset.

### 4. Serial Plans

`admit_serial_plan` accepts `policy.pacing` naming the domain group. Literal
gap 0 is still required; the registry is the authority. Each serial call is a
domain member. Its call bound (`now + timeout_ms`) is never renewed by queueing
or pacing, and the serial root stays unbounded.

### 5. Bounds

There are two distinct bounds:
- Coinbase's native 256-entry unsent queue stays a venue admission check, so it
  refuses before issuance as today.
- The engine's managed in-flight cap bounds the domain index.
They are not the same limit and are not merged.

### 6. Zero cost when off

- Everything sits behind the non-default `release-domains = ["serial-plans"]`
  feature, including the held-Bind branch, the stamping and the serial
  acceptance.
- With the feature off, the engine is byte-identical, and serial admission still
  refuses nonzero pacing.
- Only Coinbase and Hyperliquid enable it, at their own adoption PRs.

## Originals (engine, under the feature; RED on the base)

All use a registered 334 ms domain group.

1. **FIFO:**
   - three members issued A, B, C, with C prepared first: Binds come out
     A, B, C;
   - Bind for B comes only after A's release;
   - each release is at least 334 ms after the previous one.
2. **Early, equality, late:**
   - a head Claim at prev+333 is `WaitUntil(prev+334)` with no release;
   - at prev+334 it releases;
   - a late Claim releases at once;
   - the floor re-anchors at the actual release.
3. **Head terminal before release:**
   - a preparation or signing refusal, cancel, retirement or deadline on A
     advances to B;
   - B is paced from the last actual release, not from A's attempt.
4. **Stamping:**
   - an `AllRest` domain stamps public, private, background, Preflight and Plan
     Contexts;
   - a `Mutations` domain stamps writes only;
   - a Policy naming another group is `Invalid`.
5. **Construction:** an unregistered group, or a gap that differs from the
   registry, refuses engine construction.
6. **No floor spent:** a refused, stale or re-prepared claim, a duplicate, or a
   non-finite clock commits no pacing floor and no released-nonce floor (with
   `dispatch-nonce`). A re-prepare keeps the member's place, and may reserve a
   new nonce without rolling back any reserved maximum.
7. **Bound:**
   - a floor past the 20 s call deadline is `TimedOut`/NotSent;
   - queueing never extends a call deadline;
   - the serial root stays unbounded.
8. **Serial:**
   - a serial Plan in the domain: `continue_serial_plan` and `_after` calls wait
     on the floor;
   - a second command's first call queues behind the first command's last;
   - `done_empty_plan` moves nothing.
9. **Mixed owners (Coinbase `AllRest`).** One domain interleaves:
   - a public catalogue read (unsigned);
   - a background daily/candle read;
   - a command account read on the read key;
   - a history page;
   - a trading write;
   - an amend's cancellation-proof read.

   Each belongs to a different owner: command, background, Preflight and Plan
   leg. The checks:
   - Binds and releases come out in exact issuance order across owners;
   - each release is at least 334 ms after the previous one;
   - an unsigned public member waits its turn like a signed one.
10. **Mixed-owner failure handoff.** Each is native's "refuses without spending
    334", and each advances the head to the next owner's member with the floor
    still at the last actual release:
    - a signing or provenance refusal on a private member;
    - a preparation failure on a write;
    - root expiry of a Read root;
    - retirement of a Plan owner;
    - a caller cancel.

    Two further cases:
    - a later owner's member queued behind a retired owner is never stranded;
    - one owner's cancel never removes another owner's members.
11. **Delayed phases across owners:**
    - a slow Prepare or sign on the head blocks later prepared members, which
      hold Bind;
    - a delayed Bind or Claim by the host shifts every later release by exactly
      that delay and no more;
    - a fast DATA cannot pull the next release before the floor;
    - a slow DATA does not add a second gap.
12. **Linearization point.** Create Context/leg A without queueing it, then
    queue B, then queue A: Bind and release order is B, then A. A leg that is
    never queued blocks nothing, and a Stale re-prepare keeps its entry.
13. **uufub4's required negatives and evidence:**
    - **Barrier with omitted pacing:** a barrier Plan in a domain venue with
      caller `pacing: None` is still refused.
    - **Zero-gap control:** a zero-gap domain releases without waiting.
    - **Non-domain control:** a venue with no domain is unchanged.
    - **Nonce:** released versus reserved nonce evidence across WaitUntil and a
      Stale re-prepare.
    - **Repair budget:** stale repairs are cumulative and never reset.
    - **Cleanup:** mixed-owner terminals remove exactly their own index entries.
14. **GREEN controls:**
   - with the feature on and no declaration, nothing is held, stamped or paced;
   - with the feature off, serial nonzero pacing is still `Invalid`.

The venue adoption originals stay in each venue's own PR: Coinbase native order
and the 256 bound, Hyperliquid's builder prerequisites.

## Open questions for review

- (a) **Head on re-prepare. RESOLVED** (Hyperliquid consumer review). A Stale
  re-prepare keeps the SAME domain entry and head, and re-prepares within its
  original deadline. It commits neither a pacing floor nor a released-nonce
  floor. Its fresh preparation may reserve a new nonce, under section 3's
  never-rolled-back reserved maxima and the cumulative repair budget.
- (b) **Hyperliquid scope. RESOLVED.** `Mutations` is correct: native
  `pump_trades` applies 334 ms only to actual /exchange writes (Place, Cancel,
  Leverage, ScheduleCancel), and builder, info, account and history reads don't
  advance `write_at`.
- (c) **Seam. RESOLVED by uufub4.** A dedicated domain hold and index that
  shares packet and lifecycle plumbing only, as specified in section 2.
- The exact setup and SessionAuth classification for Hyperliquid is pinned with
  zs5e83 at adoption.

## Sizes and checks

- **Size:** paired Bybit, Bitget and combined on the composed tip with the
  feature off must be identical. Feature-on is measured only as an engine test
  build here.
- **Checks:**
  - engine tests with `release-domains`, and with
    `release-domains,serial-plans,dispatch-nonce`;
  - a feature-modes row;
  - clippy in every mode, wasm32 clippy and bans;
  - one PR run on a frozen head.
