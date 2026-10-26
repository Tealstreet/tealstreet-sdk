# Venue release domains: FIFO, release-paced REST (`release-domains`)

Owner: claude-dazzy3 (overseer assignment, widened to Coinbase).
Reviewers: codex-uufub4 (Plan core, nonce and release), codex-zs5e83.
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

**Ruled anchor:** the gap runs from the previous member's final release, reusing
the registered `SendPacing` floor that `commit_claim` advances.
- Coinbase: send and release coincide, so this is native parity.
- Hyperliquid: a ruled behaviour change. It is never faster than native, but
  can add latency under delayed host phases; for example, queue 0, Claim 1000,
  DATA 1001 sends the next write at 1334 instead of 1001. CALLERS records it.

There is no venue timer, no shadow floor and no queue-time ledger.

## Design

### 1. Declaration (one authority)

The venue declares, `Self: Sized`, default none, the same shape as
`socket_capabilities` / `dispatch_nonce`:

```rust
fn http_release_domain() -> Option<ReleaseDomain> where Self: Sized { None }
pub struct ReleaseDomain { pub pacing: Pacing, pub members: DomainMembers }
pub enum DomainMembers { AllRest, Mutations }
```

- `pacing.group` must be a group the venue registers in `http_pacing()`.
  Engine construction refuses a domain whose group isn't registered, or whose
  literal gap differs from the registered one.
- The **engine stamps** the domain's pacing onto every member Context it issues
  for this venue. That covers command, background, discovery, Preflight call,
  Plan leg and serial call Contexts. A member's own `Policy.pacing` must be
  `None` or that same group, otherwise `Invalid`, so no Policy can bypass the
  domain or join a second one.
- Coinbase declares `AllRest`; Hyperliquid declares `Mutations`.

### 2. FIFO, enforced at Bind

- Members enter the domain queue in issuance order: the order of
  `request_managed` / `request_signed_managed` / Plan-call issuance.
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
- Pacing is checked before any nonce floor commit, so no nonce is reserved for a
  `WaitUntil` (uufub4's interaction).

### 4. Serial Plans

`admit_serial_plan` accepts `policy.pacing` naming the domain group. Literal
gap 0 is still required; the registry is the authority. Each serial call is a
domain member. Its call bound (`now + timeout_ms`) is never renewed by queueing
or pacing, and the serial root stays unbounded.

### 5. Bounds

The domain queue is capped at the existing managed-request capacity. Coinbase's
native 256 stays a venue admission check, so it refuses before issuance as
today.

### 6. Zero cost when off

- Everything sits behind a non-default `release-domains = ["http-plans"]`
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
6. **No credit spent:** a refused, stale or re-prepared claim, a duplicate, or a
   non-finite clock spends no floor and no nonce (with `dispatch-nonce`). A
   re-prepare keeps the member's place.
7. **Bound:**
   - a floor past the 20 s call deadline is `TimedOut`/NotSent;
   - queueing never extends a call deadline;
   - the serial root stays unbounded.
8. **Serial:**
   - a serial Plan in the domain: `continue_serial_plan` and `_after` calls wait
     on the floor;
   - a second command's first call queues behind the first command's last;
   - `done_empty_plan` moves nothing.
9. **GREEN controls:**
   - with the feature on and no declaration, nothing is held, stamped or paced;
   - with the feature off, serial nonzero pacing is still `Invalid`.

The venue adoption originals stay in each venue's own PR: Coinbase native order
and the 256 bound, Hyperliquid's builder prerequisites.

## Open questions for review

- (a) **Head advance on re-prepare.** Does a Stale re-prepare keep the head
  (blocking the queue until it re-signs, as native's sign-at-dispatch
  effectively does), or yield to the next prepared member? I propose keeping it,
  for FIFO.
- (b) **Hyperliquid scope.** Is `Mutations` the right member set, or does its
  native 334 ms also bind something non-trading? My reading of `trading.rs` says
  writes only.
- (c) **Seam.** Is reusing the barrier-member held-packet path for a held Bind
  acceptable (uufub4), or should the domain have its own hold beside it?

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
