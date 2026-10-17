# SPEC — the parity manifest (canonical; the overseer owns this file)

Two agents build the two halves of the manifest system and **must not invent
their own format**. This is the contract between them. If you think it is wrong,
tell the overseer; do not change it unilaterally.

Background: `docs/plans/SAFE_CEX_RUST_PORT_v1.md` §5.

## 1. Layout — one manifest file per TypeScript source file

```
sdk-rs/parity/manifest/<path under packages/safe-cex/src>.toml
```

e.g. `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts`
→ `sdk-rs/parity/manifest/exchanges/bybit/bybit.exchange.ts.toml`

**One file per source file, not one big manifest.** 26 venue agents will be
adding rows in parallel; a single file would conflict on every merge. Keep the
`.ts` in the manifest filename so the mapping is mechanical in both directions.

## 2. Row schema

```toml
[file]
ts = "packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts"
rust_files = [ "safe-cex-venue-bybit/src/exchange.rs" ]

[[symbol]]
ts = "BybitExchange.placeOrder"
kind = "method"
layer = "mirror"
rs = "safe-cex-venue-bybit/src/exchange.rs::BybitExchange::place_order"
status = "ported"
note = ""
```

| Field       | Owner        | Values                                                                                               |
| ----------- | ------------ | ---------------------------------------------------------------------------------------------------- |
| `[file].ts` | TS extractor | repo-relative path of the source file |
| `[file].rust_files` | agent | **required once any row in the file is mapped** — the exact `<crate>/<path>.rs` files this file's rows may map into, as an array of strings. One entry per file, no wildcards, no globs. §12 |
| `ts` | TS extractor | symbol path **within the file** — see "Naming the awkward kinds" below; the forms are a convention, not examples |
| `kind` | TS extractor | `function` · `method` · `field-fn` · `constructor` · `accessor` · `class` · `const-fn` · **`const`** · `type` · `enum` |
| `layer` | agent | `mirror` (line-for-line) · `adapted` (function-for-function) — plan §5.3. The extractor writes it **empty**; it is never defaulted to `mirror`. Required exactly when `rs` is non-empty (§10). |
| `rs`        | agent        | `<crate>/<path>::<Item>` or `<crate>/<path>::<Impl>::<method>`, **or an array of those** when one TS symbol needs several Rust items (§10); empty while `pending` |
| `status`    | agent        | `pending` · `ported` · `divergent` · `not-applicable`                                                |
| `note` | agent | **required for `divergent` and `not-applicable` only** — see §7 |

### Naming the awkward kinds

- `constructor` → `ts = "Klass.constructor"`. **Extract it.** A constructor is
  load-bearing behaviour here, not boilerplate: `BaseExchange` supplies branded
  inert defaults for `start`, `placeOrder` and `fetchHistoricalFills`, and
  *derived class fields replace the branded function during construction* —
  which is the mechanism the whole metadata-capability baseline rests on. See
  `packages/safe-cex/CLAUDE.md` → "Metadata capability non-fallback baseline".
- `accessor` → `ts = "Klass.get foo"` / `"Klass.set foo"`. Extract both halves
  separately; a getter and a setter are two units of behaviour.
- **A static member is `Klass.static foo`**, distinct from the instance
  `Klass.foo`. `static call()` beside `call()` is legal TypeScript, so a scheme
  that names both `Klass.call` collides — and if the collision is a hard error,
  one added static stops the extractor working across the whole repo.
- **A `#private` member keeps its hash: `Klass.#foo`**, distinct from
  `Klass.foo`. Stripping the `#` collapses two different members onto one name.
- `enum` → `ts = "OrderSide"`, the declaration's own name.

`kind = "field-fn"` is the one that matters most: exchange methods are instance
fields (`fetchIncomeHistory = async () => {}`), which a naive "exported
declaration" walk misses entirely. That same property is why the WOO hedge-side
bug was untestable — see `packages/safe-cex/src/exchanges/CLAUDE.md`.

## 3. The TS extractor is a RECONCILER, not a regenerator

This is the single most important behavioural requirement. On every run it:

- **preserves** `rs`, `status`, `note` and `layer` on every row whose `ts` +
  `kind` still match. Clobbering an agent's porting work is the one
  unrecoverable bug this tool can have.
- **adds** newly-seen symbols as `status = "pending"`, empty `rs`/`note`.
- **does not delete** a row whose symbol has vanished from the source. It
  reports it as `orphaned` and exits non-zero in `--check` mode. Deleting a row
  is a deliberate act a human or agent takes, because a vanished TS symbol may
  mean "renamed" (the Rust side needs updating) or "removed" (the Rust side
  needs deleting), and the tool cannot tell.
- is **deterministic**: stable row order (source order), stable key order,
  trailing newline, so a no-change run produces a zero-byte diff.
- **exits non-zero only in `--check` mode.** A write run that finds orphans
  reports them and exits 0; failing there makes `yarn parity:extract` red
  during any rename-in-flight, which is exactly when someone needs to run it.

## 4. Scope of extraction

In: a **declared list of roots**, not one hardcoded prefix. Today:

```
packages/safe-cex/src
packages/script-api-core/src/order
```

**The list grows deliberately, one phase at a time, and never by accident.** It
started as the single `packages/safe-cex/src` prefix, which was wrong the moment
P1 began: `nativePositionSize` and `breakevenPrice` live in
`packages/script-api-core/src/order/` and are squarely in the port's scope — the
plan's §3.2 dependency closure includes `script-api-core` — so the port had
symbols with nowhere to put a row.

Two consequences worth stating, because the obvious workaround is broken:

- **Do not hand-write a row for a file outside the roots.** The extractor never
  visits it, so the row has no source symbol to reconcile against and gate 1
  either orphans it or behaves undefinedly. A manual row does not extend
  coverage; it breaks the gate that measures coverage.
- **Adding a root adds its whole subtree**, so widen at the narrowest directory
  that contains what is actually being ported. `script-api-core/src/order`, not
  `script-api-core/src` — the latter is 9,488 LOC of which P1 touches two files.

Out: `*.test.ts`, `*.test.tsx`, `__tests__/**`, `__snapshots__/**`,
`env.d.ts`, `*.d.ts`, `perfFixture.ts`.

Extract: top-level `export`ed functions, classes, consts holding a function,
type aliases/interfaces that describe wire or domain shapes, **and `enum`
declarations**; plus **every method and function-valued field of an exported
class**, public or private. A private method is still a unit of behaviour that
must exist on both sides.

Do not extract: imports, re-exports, plain data consts, enum MEMBERS.

### Reachability, not the `export` keyword, decides what is in scope

This clause was originally written as "exported class", and that wording lost
**9 module-private classes and 61 of their members** — 70 rows, which is what the corrected baseline recovered (an earlier revision of this spec said "8 classes and 92 members" and inflated every per-class figure; those came from a first-pass review and were propagated without checking — a spec's numbers are the only record a later reviewer checks against, so a wrong one is worse than none), including two that would have been catastrophic:

- `webworker/twap/TwapManager.ts` declares `class TwapManager` and exports
  `const twapManager = new TwapManager()`. The class is not exported and the
  instance is not callable, so neither rule fired and the manifest file for the
  **entire TWAP engine was empty**.
- `utils/virtualClock.ts` is the same shape, **17 rows** of its members (the
  other 7 are data fields — `started`, `timeoutId`, `server`, `client`,
  `offsetMs`, `syncInFlight`, `syncListeners` — correctly excluded). `virtualClock.getCurrentTimeMs()`
  is where every signed request's timestamp comes from — the exchanges doc says
  never to use `Date.now()` for one, because of HMAC `recvWindow` enforcement.
  A port that silently omitted it would fail at runtime on signature rejection,
  nowhere near the cause.

Also lost: `HttpRequestStatsTracker` (6), `AsterDEXVirtualClock` (18),
`TealstreetLighterSignerAdapter` (10), `JsonDuplicateKeyScanner` (9) and three
error classes.

**The test is whether behaviour is reachable from outside the module** — through
an exported instance, an exported factory, an exported subclass, or the class
itself. If it is, its members are rows. A module-private helper class that
nothing outside can reach is not.

### An exported object literal of functions is a provider, and its functions are rows

"Function-valued field" was implemented for class members only, which lost
**4 consts carrying 66 function properties**:

| const | file | fn props |
| --- | --- | --- |
| `exchangeRegistry` | `exchanges/registry.ts` | 47 |
| `directTransport` | `transport/direct.ts` | 9 |
| `tealstreetTransport` | `transport/tealstreet/tealstreetTransport.ts` | 9 |
| `WEEX_RETRY_CONFIG` | `exchanges/weex/weex.api.ts` | 1 |

`transport/direct.ts`'s manifest file was **empty**, though the file holds two
load-bearing guards — the refusal of an account carrying `tealstreetProxy`,
which is what makes a missed transport registration fail loudly instead of
tripping venue IP allowlists, and the warn-once on `tealstreetWsRoute`.
`exchangeRegistry` is the whole venue registry.

Name them `objectName.propName` with `kind = "field-fn"`.

### A handle is decided by the INITIALIZER, not by the type's provenance

`AxiosInstance` has call signatures, so a type-driven callable test tags
`AsterDEXExchange.primaryAPI`, `#secondaryAPI`, `#unlimitedAPI`, `#publicAPI`
and three Polymarket `xhr` fields as `field-fn`. The checker is right and the
semantics are wrong: those are client handles the port will not reimplement.

**But "its type is declared outside this package" is the wrong rule, and taking
it literally removes 19 fields rather than 7.** It would also drop
`BaseExchange.on` / `once` / `off` (TinyEmitter) and nine lodash
`DebouncedFunc` / `ThrottledFunc` fields. Those are behaviour and must stay.

Decide on what the initializer produces:

1. **A function literal — arrow or function expression — is a row**, including
   when it is wrapped by a higher-order helper such as `debounce`, `throttle`,
   `memoize` or `once`. The wrapper is incidental; the wrapped body is this
   package's behaviour, and the interval is itself load-bearing. Nine lodash
   fields are rows for this reason.
2. **A method re-exposed from another object — delegation, `.bind` — is a row,
   `layer = "adapted"` — which the PORTING AGENT sets, not the extractor, since §2 says the extractor writes `layer` empty and never pre-judges it.** It is public API surface a consumer calls:
   `exchange.on('error', …)` has to exist on the Rust side too, even though the
   implementation there is also delegation. `BaseExchange.on`/`once`/`off`.
3. **A third-party client instance is NOT a row**, when the field is callable
   only because that external type happens to carry call signatures. The port
   will construct a `reqwest` client instead; nothing is being ported into the
   field. The seven `AxiosInstance` fields are the whole set.

So: 7 excluded, 12 retained. If a future case fits none of the three, the
question to ask is "will the port reimplement what this field holds, or
construct something different in its place" — behaviour is a row, a seam is not.

**An enum is a row; its members are not.** This was ambiguous in the first
version of this spec and the first extractor read it as "no enums at all",
which silently omitted `OrderSide`, `OrderType`, `OrderStatus`,
`OrderTimeInForce`, `PositionSide`, `MarginType` and fifteen more — 21
`export enum` declarations repo-wide. They are precisely the wire and domain
shapes this clause means to include, every one of them becomes a `pub enum` on
the Rust side, and the invention gate would then fire on each with no
legitimate exemption, since §5 says an exemption may never stand in for
behaviour TypeScript does not have.

## 5. The gates

Five, not the three this section was originally written for: coverage, invention and
status, plus name correspondence (§11) and provenance (§12).

| Gate          | Rule                                                                                                                                 | Exit                         |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------- |
| **Coverage**  | a run in `--check` mode that would add or orphan a row fails                                                                         | non-zero, naming each symbol |
| **Invention** | every `pub` item in the `sdk-rs` crates appears as some row's `rs`, unless exempted                                                  | non-zero, naming each item   |
| **Status** | `divergent` or `not-applicable` with an empty `note`; or `status = "ported"` with an empty `rs`; or a row whose `layer` is empty while its `rs` is not, or non-empty while its `rs` is empty; or `not-applicable` with a non-empty `rs`; or an `rs` that does not resolve to a real Rust item; or an array `rs` with an empty `note` | non-zero, naming each row |
| **Name correspondence** | a `ported` or `divergent` row whose Rust leaf is not the expected transformation of its TS leaf, unless its `note` names the exact `rs` target — §11 | non-zero, naming the row and both paths |
| **Provenance** | a row whose Rust FILE is not declared for its `[file].ts` — §12 | non-zero, naming the row and the undeclared file |

Invention exemptions live in `sdk-rs/parity/exempt.toml`, one entry per item
with a **required** reason. Legitimate exemptions: trait impls required by a
derive, `main`, test helpers, generated code, the FFI boundary's own glue. An
exemption is not a way to add behaviour TypeScript does not have — that is what
`status = "divergent"` plus a note is for.

## 6. Commands (root `package.json`; the Rust agent adds all four)

```
yarn parity:extract        # reconcile the manifest from TS source
yarn parity:extract:check  # same, but fail instead of writing  (gate 1)
yarn parity:check          # gates 2 and 3
yarn parity:report         # human-readable progress, per file and per crate
```

`parity:report` prints, per manifest file and aggregated per directory:
`ported / total`, plus counts of `divergent` and `not-applicable`. It is the
project's real progress metric — LOC estimates are not.

## 7. Why `pending` needs no note

`pending` with an empty `rs`, `note` and `layer` is the legal resting state of
an unported symbol, and it is what the extractor writes for every row on the
first full extraction. **The status gate must pass on a manifest that is
entirely `pending`.**

A gate that fails on thousands of untouched rows from the moment it is born is
a gate everybody learns to ignore, and then it is not protecting anything. The
three gates have distinct jobs and must not be collapsed:

- **coverage** catches a TS symbol with no row at all — the port falling behind
  `master`;
- **invention** catches Rust with no TS counterpart;
- **status** catches a row whose own fields contradict each other.

"Not ported yet" is not a contradiction. `divergent` and `not-applicable` are,
unless justified — those are the two that demand a note.

## 8. Two rules the gates must enforce that §2 only implied

Both were found by an adversarial review of the gate implementation on
2026-10-11, and in both cases the code was not wrong — this spec was silent.

### A `pending` row's `rs` must be empty, and must not discharge the invention gate

§2 says `rs` is "empty while `pending`". That was written as a description and
read, reasonably, as advice. It is a RULE:

- `status = "pending"` with a non-empty `rs` is a **status error**;
- the invention gate's set of mapped Rust items is built from **non-`pending`
  rows only**.

Otherwise a bare `pending` row — no `layer`, no `note`, no claim that any work
happened — becomes the cheapest way to silence an invented Rust item, cheaper
than the `divergent` + required note this spec reserves for exactly that. The
gate would then be discharged by an assertion that nothing has been done.

### A Rust item claimed by more than one row must be declared

`parity-report`'s `ported / total` is the project's real progress metric, so it
must not be inflatable by pointing N TypeScript symbols at one Rust function.

There is a legitimate case — one Rust function genuinely covering two
TypeScript symbols — so this is not a flat prohibition:

- a duplicate `rs` among `ported` rows is an **error unless every row sharing it
  carries a note**, the same bar as `divergent`: a deliberate collapse has to be
  justified in writing;
- `parity-report` additionally prints **distinct Rust items claimed** beside
  `ported / total`, so any collapse is visible in the metric rather than hidden
  inside it.

### And one rule that is honestly unenforceable

"An exemption is never a way to add behaviour TypeScript does not have"
(§5) cannot be machine-checked — it is a human reading a `reason` string. The
enforceable half IS enforced and should stay that way: `reason` is required at
parse level rather than by a lint, globs and wildcards are impossible, and a
stale exemption is reported. Treat the semantic half as a review obligation,
not a gate.

**The same is true of `note` CONTENT, and it was argued out rather than
assumed.** A round-2 review found that `note = "x"` satisfies the note
requirement at exit 0, so every `divergent` and `not-applicable` row in the port
rests on a single character. A substance check was considered and **deliberately
rejected**, on two counterexamples that kill it from both sides: `"Browser-only."`
legitimately explains a `not-applicable` row and would fail any length or
token-count threshold, while `"This is intentionally different"` passes every
such threshold and explains nothing. Rejecting a denylist of `x` / `todo` / `n/a`
would catch those exact placeholders and nothing else, while making a green gate
imply a validation it does not perform — which is the failure this whole gate
program exists to stop.

So: **the gate establishes that an explanation was supplied, never that it is
adequate.** Judging whether a note states what differs, why, and whether the
named implementation actually supports the claim is a review obligation. That
matters most where a note **exempts a mapping from correspondence checking**
(§10, hole #10), because there the note is the only thing standing between a
`ported` row and a wrong mapping. Measured for reference, not as a rule: the
corpus had 34 non-blank notes, all at least 55 characters.

## 9. Hole #7 — reachability applies to FUNCTIONS too, not only classes

Found 2026-10-11 by an adversarial review of the extractor's own fixes, and it
is the seventh hole in this document.

§4's heading says *"Reachability, not the `export` keyword, decides what is in
scope"* and its test is *"whether behaviour is reachable from outside the
module"*. But §4's Extract list still said "top-level **`export`ed**
functions". The implementation resolved the contradiction by taking the wide
reading for classes and the narrow one for functions, which is defensible as a
reading and indefensible as a result:

- `safeCexAxiosCreate.ts`: non-exported `class HttpRequestStatsTracker` yields
  7 rows, while non-exported `recordRequestOutcome`,
  `isTealstreetProxyAuthFailureBody`, `stringifyResponseBody` and
  `maybeInstallRemoteSigningAdapter` yield **none**.
- Repo-wide that is **589 non-exported top-level function-valued consts plus 49
  non-exported function declarations — 638 symbols, about 11% of the corpus.**

**Ruling: they are rows.** The same test decides both, and
`recordRequestOutcome` is exactly the kind of thing this manifest exists for —
it increments `store.requestStats` from the axios interceptors, and an agent
fixing a bug in it must be able to find the other copy.

Three things make the cost acceptable:

- **`pending` needs no note (§7)**, so 638 new rows are 638 `pending` rows and
  there is no upfront annotation burden. They are annotated when someone
  actually ports that file.
- **A helper the Rust side INLINES is not a problem, it is information.** Give
  it `status = "divergent"` and a note saying where the logic went. That is the
  same habit the repo's own rule about editing a guard asks for: leave the next
  reader the delegation rather than a hole.
- A bigger denominator makes `ported / total` less flattering and more true.
  That trade is always worth taking.

**Consistency is the real argument.** Including module-private classes while
excluding module-private functions is worse than either choice taken whole,
because nobody can predict which half a given symbol falls in.

## 10. Holes #8 and #9 — `layer` was tied to a status, and one TS symbol could not map to two Rust items

Both found by a sub-agent attacking this spec against the `safe-cex-crypto`
chunk. Both were holes in the spec, not in the chunk.

### #8 — `layer` is required by `rs`, not by `status`

The status gate used to demand a `layer` only on `status = "ported"`. So a
`divergent` row with a blank `layer` and a note passed every gate, exit 0 —
verified. `divergent` is precisely a status that asserts Rust exists and
deliberately differs, which is when knowing whether it mirrors or adapts
matters most.

Tying the requirement to the status label was the mistake. `layer` describes how
a Rust item relates to its TypeScript original, so it is meaningful exactly when
there **is** a Rust item:

- `rs` non-empty -> `layer` **required**. Covers `ported` and `divergent`.
- `rs` empty -> `layer` **must be empty**. Covers `pending`, and
  `not-applicable`, which additionally must have an empty `rs` — a row saying
  "this does not apply to Rust" cannot also name Rust.

This is enforceable from the row alone and needs no status table.

### #9 — an array `rs`, for one TS symbol implemented by several Rust items

`rs` was a single string, and §8 covered only the N TS -> 1 Rust direction. The
reverse is ordinary in Rust: a single TypeScript type whose fields a port splits
into two structs, or a TS interface that becomes a trait plus its options type.
That was **inexpressible**, so the only legal way to record the extra Rust item
was an invention exemption — which is how three behaviour-carrying types ended
up exempted instead of mapped, invisible to the invention gate and absent from
the progress metric. The spec forced that.

So `rs` may be an array:

```toml
[[symbol]]
ts = "RemoteSignerResponse"
kind = "type"
layer = "adapted"
rs = [ "safe-cex-crypto/src/lib.rs::WsAuthResponse", "safe-cex-types/src/lib.rs::RemoteSignerResponse" ]
status = "ported"
note = "Split: the envelope lives in safe-cex-types; the crypto crate holds the WS-auth projection, whose skip_serializing_if reproduces the absent-key {id,error} vs {id,wsAuthSignature} shape."
```

**Write the array on ONE line, exactly as above.** `smol-toml` re-emits `rs = [ "a", "b" ]` single-line, so a multi-line array is reformatted by a reconcile and `yarn parity:extract --check` then fails the row **for formatting alone**, reporting `changed:` with `0 added, 0 orphaned` — which reads like a coverage failure and is not one. This example was multi-line in the first version of this section and a reviewer found it failing the gate; the serializer is right and the example was wrong.

Rules, because an array is also the obvious way to silence the invention gate by
dumping unrelated items into one row:

- **An array `rs` requires a `note`**, naming why the TS symbol needs more than
  one Rust item and what each one holds. Same shape as the rule in §8 for a Rust
  item claimed by several rows.
- The invention gate discharges **every** item in the array; each must resolve.
- `layer` applies to the row as a whole. If two Rust items genuinely differ in
  layer, that is two TS symbols being conflated — split the rows instead.
- A single string stays valid and stays the normal case. Reach for an array only
  when a split already happened for a reason you can state.

**An exemption is never the right home for a behaviour-carrying type.** If an
exemption's reason names a real TypeScript symbol, that symbol has a row, and
the item belongs in it — now that a row can hold it.

## 11. Hole #10 — name correspondence, and what it does NOT cover

Missing from this spec until a reviewer pointed out that an agent hitting
`Rust item path must be ...; differing names require note` had no contract to
read — in a document that opens by telling agents not to invent their own
format.

**The rule.** A `ported` or `divergent` row's `rs` must be the expected
transformation of its `ts`, compared on the **whole qualified path**, not the
leaf alone. `BybitExchange.placeOrder` -> `BybitExchange::place_order`. A row
whose names legitimately differ needs a `note` **that contains the exact `rs`
target as a whitespace-delimited token** — prose alone does not exempt it.

The transformation is more than camelCase-to-snake_case, and an agent who
assumes otherwise gets rejected:

- `.` -> `::` on the qualifier;
- camelCase -> snake_case on a **member**;
- `class`, `type` and `enum` leaves are left **verbatim** — `OrderSide` stays
  `OrderSide`, NOT `order_side`;
- `constructor` -> `new`;
- the `static `, `get `, `set ` and `#` markers are stripped, which is why the
  two halves of an accessor collapse onto one path (see below);
- an `_` is inserted after a digit.

**Why it exists.** Before it, `rs` was checked for EXISTENCE only, so swapping
two rows' Rust items passed. A `ported` row was not evidence that *that* Rust
item implements *that* TypeScript symbol, which is most of what this manifest is
for.

**Three holes this section used to describe are CLOSED.** They are recorded
because the fixes are the reason the current rules look the way they do:

- the comparison discarding crate and file, which left **639 rows nominally
  interchangeable** — now BOUNDED by the provenance rule in §12. Two honest
  caveats, because this was overstated here before: **634 of those 639 rows are
  `pending` with an empty `rs`**, so they were never live, and among the mapped
  rows there are zero shared expected paths even without provenance; and
  provenance *bounds* rather than closes, because the bound is **self-declared**
  (§12). What it defuses is a latent 639, not an active one;
- only `ported` rows being checked — the guard now covers `divergent` too, which
  is the status that most needs it;
- an array `rs` being categorically exempt via its `note` — closed by requiring
  the note to name the exact `rs` target, so it can no longer exempt by merely
  existing.

**What it still does not cover.** These pass today and are known:

- **`static`, `#` and `get `/`set ` all collapse** onto one expected path, so
  the two halves of an accessor are interchangeable; there are four real pairs
  and 114 accessor rows. In the other direction the idiomatic Rust setter
  (`set orders` -> `set_orders`) is REJECTED and needs a declaration, which
  pushes accessor rows toward escapes.
- **A note that names the target discharges correspondence unconditionally.**
  That is the escape working as designed, but it means the gate cannot judge
  whether the rename is RIGHT. The first six uses of the mechanism in this port
  contained one wrong mapping — `bybitTimeUrl` -> `fetch_venue_time`, where the
  Rust holds no URL, no proxy resolution, no timeout and no parse, and the note
  said so. **Judging a declared rename on its merits is a review obligation**,
  exactly like a `note`'s content in §8.

**The coverage number, re-measured, having now been wrong twice.** The rule runs
on **146 mapped rows** — `ported` AND `divergent`, since the guard covers both —
of which **140 `rs` entries are checked** and **6 are note-escaped**, so the
audited population is **2.2% of 6,259**. The figures this paragraph carried
before (132 / 20 / 112 / 1.8%) were wrong on the count *and* on the criterion:
exemption no longer means "carries a note", it means "the note names the exact
`rs` target", which only 6 rows do. The zero-mismatch tail is evidence the gate
is young, **not** that the rule is strong. Do not quote the corpus size as the
audited population — that claim went into a PR body and had to be retracted, and
§4 of this spec says a wrong number is worse than none, which applies to this
paragraph twice over.

### The `note` overload, and what was done about it

A single `note` used to disable **three** independent rules — correspondence,
the shared-claim rule (§8) and the array-split requirement (§10) — so a note
written to explain a shared claim silently switched off the name gate too.

Of the two ways out, the second was taken: **a note discharges correspondence
only if it contains the exact `rs` target as a whitespace-delimited token.** The
shared-claim and array-split rules stay independent of it. Prose alone no longer
exempts anything.

**The residual is now one character, not zero prose.** A note of bare tokens
alone is rejected (`array rs requires note explaining the split beyond Rust
target tokens`), including backticked or comma-punctuated tokens. But
`note = "x <token> <token>"` passes. So the spec's own named abuse case — one
row, ten unrelated public items across two crates, an empty `exempt.toml` —
still passes with a single alphanumeric character in front of the tokens. That
is consistent with §8's deliberate refusal to judge note CONTENT; it is simply
the honest bound, and it is lower than "zero prose" implied.

## 12. Provenance — a Rust file must be DECLARED for the TypeScript file that maps into it

Added after a merge gate measured that name correspondence alone left **639 rows
(10.2%) mutually interchangeable**: it compares everything after the first `::`,
so `create_api` x27, `to_number` x20 and `is_record` x13 were all one expected
path, and `BinancePrivateWebsocket::on_message` was the same path across the
binance usdm, coinm and spot files. A free function has no qualifier at all.

**Each manifest file declares the Rust files its rows may map into**, and a row
whose `rs` names a file outside that set fails the gate. Re-measured with the
rule in place, interchangeability is **zero live pairs**.

**It lives in the per-source manifest file, not in one central table.** A single
central file would be edited concurrently by every chunk agent, which is exactly
the merge-conflict shape §1 gives as the reason manifests are per-source-file to
begin with. The first implementation was central and would have red-lit five
in-flight chunks at once — roughly 937 mapped rows across 125 TypeScript files,
none of them declared.

**A rename declaration does not override provenance.** The note-names-the-target
escape in §11 discharges the NAME rule only; the file set still binds. That
ordering is deliberate: a wrong name with a stated reason is a judgement call,
while a row reaching into another TypeScript file's crate is a mapping error.

Known limits, so nobody reads more into it than it gives: nothing bounds how
many Rust files one TypeScript file may declare, nothing verifies a declared
file exists, and nothing reports a declared file that no row maps. A declaration
set that widens back toward the pre-fix state produces no signal — unlike §8's
stale exemptions, which are reported.

## 13. Exported VALUE constants are in scope, as `kind = "const"`

Added after a merge gate found the coverage gate structurally blind to them:
`extract.ts` admitted a `const` only when `callable(checker, ...)` was true, so
an exported non-callable data const was invisible to every gate. `ALL_EXCHANGES`
in `utils/tickIntervalOverride.ts` was absent from its manifest AND from
`exempt.toml`, while the Rust side carried a bare `"*"` literal — and nothing
could notice. That is the same shape as a `pub(crate)` Rust module being outside
the invention gate: the hole was in what the gate could SEE, not in what anyone
wrote.

**Measured before the gate was allowed to fail on it**, which is the rule for
any widening: **542 additions across 104 files, taking the corpus from 6,259 to
6,801 rows.** Every addition is `pending`, and no existing annotation changed.

A data const is in scope because the Rust side still has to hold the same value,
and a wrong one is exactly as load-bearing as a wrong function: `ALL_EXCHANGES`
is the reserved key of a registry, and `"*"` drifting silently is a routing bug
with no symptom at the call site.

## Reference bug markers

`ref-bug:` complements `ts-bug:` when the defective behavioral reference is
Go/WASM or another non-TypeScript implementation. Apply the same note discipline:
name the reference and concrete defect, state whether Rust preserves it or
deliberately diverges, identify the owning manifest row, and pin the behavior
with an independent oracle and negative control. A deliberate security refusal
must be `divergent`, not silently described as reference parity.

<a id="shared-utf8-string-boundary"></a>

## 14. Shared UTF-8 string boundary

Canonical representation declaration: **`shared-utf8-string-boundary`**.
**Cite it. Do not restate it per row.**

A row whose **entire** cause of divergence is that the shared Rust domain uses
UTF-8 strings where TypeScript uses UTF-16 cites
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and keeps only its
own mapping or behaviour notes — which, for a pure representation case, is
nothing. A MIXED row cites it *alongside* its independently justified local
choices or findings.

bitget's taxonomy audit measured 24 such rows on **one** venue. Written longhand
across 26 venues that is several hundred near-identical notes, and the cost is
not the duplication: the day this representation changes, every one of them is
wrong and nobody can find them all. A citation makes it one edit. It also stops
the count lying about a venue — bitget's 43 is 19 real choices plus 24 instances
of one shared fact, and read as 43 it looks like the worst parity ratio in the
project when it is not.

### The fact being cited

A JavaScript string is a sequence of UTF-16 code units with **no well-formedness
requirement**. A lone surrogate — any code unit in `U+D800..U+DFFF` without its
pair — is a legal, observable JavaScript string value, **including one decoded
from a valid JSON escape**. It round-trips through property keys, compares,
concatenates, and survives `JSON.stringify` as an escape.

The shared Rust domain, transport and JSON interfaces use `String`, `str` and
`serde_json::Value`. Those represent Unicode scalar strings and **cannot retain a
lone surrogate at all**. So for every ported function whose domain or range is a
string, there exist JavaScript inputs with no Rust representation. The representation restriction is shared; the bucket is determined row by row
by the post-boundary observation in [§19](#forced-surrogate-conflict). A citation
alone does not establish that a diagnostic is program-observable.

**A valid JSON escape rejected by Rust is a representation refusal, not invalid
JSON.** Program-observable values must not be silently replaced. The narrow
diagnostic-only replacement exception, its price, and its consumer evidence are
spelled out in [§19](#forced-surrogate-conflict).

### The shared model choice and the row bucket are separate

Choosing the common UTF-8 model is `CHOSEN`: Rust can implement UTF-16
code-unit strings, WTF-8 or a broader value model. That does not make a lone
surrogate representable inside the required `String`/`str`/`Value::String`
surface. Under the observability ruling in [§19](#forced-surrogate-conflict), a
row is `FORCED` only for an exact program-observable unit that this surface
cannot retain. Diagnostic-only substitution is `CHOSEN` and priced. Neither
bucket means Rust as a language cannot implement a different shared model.

Changing one venue alone would introduce a second string/value model and a
conversion boundary at every edge, while the shared store, transport and typed
consumers still require UTF-8 — so local mirroring moves the mismatch to those
boundaries and adds parallel state instead of giving end-to-end preservation. It
would also push a surrogate-capable type across the whole public surface of the
port, including the wasm boundary, where it must become a JavaScript string
again anyway. A coordinated replacement would change every public string edge, not merely
one venue function.

That explains the shared model choice; it does not replace the row-specific
observation required by §19. A coordinated shared-model change can remove the
restriction, and both declarations must then be revisited.

### Witness discipline

Maintain double-escaped witnesses that preserve the TypeScript acceptance and
output alongside the Rust ingress rejection. Ordinary scalar Unicode, astral
pairs and whitespace stay in the positive corpus, so the refusal is pinned to
lone surrogates and cannot quietly widen.

A shared representation change must audit **every** reference to this identifier
and update its witnesses. Per-row references are the inventory of what such a
change has to touch — they are not permission to erase the rows.

### What this declaration does NOT cover

Citing it is a claim about **lone surrogates and nothing else**. These remain
separate rows with their own notes:

- **Normalization** — NFC/NFD or any Unicode normalization difference is not this.
- **Ordering and collation** — `JsRootCollator` exists precisely because byte
  order is not JavaScript order. Never cite this for an ordering divergence.
- **Case mapping** — locale- or Unicode-version-dependent case behaviour.
- **Any row whose inputs are also restricted on other grounds** — e.g. a row that
  additionally excludes exotic JSON inputs. Those exclusions are the row's own and
  must be stated; kucoin's string exclusions are this shape.
- **A measured zero-divergent manifest.** Reporting zero divergent rows is a
  statement about the rows currently measured. It is not a claim that lone
  surrogates are representable.

<a id="closed-typed-output-boundary"></a>

## 15. The closed typed output boundary

Canonical representation declaration: **`closed-typed-output-boundary`**.
**Cite it. Do not restate it per row.**

A row whose **entire** remaining divergence is that an inherited-member table hit
cannot be carried into a closed typed field of the shared model cites
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`. A MIXED row cites
it *alongside* its own causes.

### The fact being cited

[`shared-utf8-string-boundary`](#shared-utf8-string-boundary)'s sibling, and the
second half of the inherited-member defect. `TableHit` reproduces the **branch**
decision faithfully — `is_truthy`, `is_nullish` and presence all answer exactly
as JavaScript does, so every `||`, `??` and `!` guard takes the same path. What
it cannot do is put the hit into the field the mapper returns.

`safe-cex-types::Order` carries type/status/side as `ModelField` values;
`Position.side` uses the same carrier. Absent properties and own undefined are
represented directly, and JSON omits both. This boundary does not justify
refusing an undefined enum lookup or a missing Order.price. The typed value
variants remain closed enums. `DisplayedPosition` declares typed ticker and
market fields. Those typed values are consumed by the store, worker, UI and
OEMS layer. In TypeScript the same field holds an inherited **function**.

So a mapper can take the right branch and still be unable to return what
TypeScript returns. Measured independently on woo (`mapLimitOrder`,
`mapAlgoOrder`), bybit (inverse position `side`), binance (historical order
fields), bitget (`mapOrder`) and the webworker (fake-position ticker/market).

### Why this is `CHOSEN` and never `FORCED`

Rust **could** carry it. A marker variant, or a raw sidecar field holding the
original wire key plus an inherited-member tag, would let the mapper return a
record at all instead of refusing.

We decline. Admitting an inherited-member variant into the closed shared enums
would force **every** consumer — store, worker, UI, OEMS — to handle a case that
arises only from a defect, and would do it at the exact boundary whose closedness
is the reason the typed model exists.

**That is the why-mirroring-would-be-worse sentence for all of these rows.**

### The divergence is REAL and survives serialization — do not claim convergence

An earlier revision of this section claimed a refused mapping and an omitted key
were the same thing to a reader of the serialized record. **That is false**, in
two separate ways, both measured against live Node. Never write it.

**1. Refusing the mapping is not omitting the field.** Rust returns `Err` for the
**whole** record. TypeScript returns the record with only that one field missing:
`JSON.stringify({side: obj['constructor'], id: 1})` is `{"id":1}` — a complete
`Position` or `Order` that merely lacks `side`. A consumer gets a usable record
from TypeScript and nothing at all from Rust. That is a larger divergence than
the field itself.

**2. `__proto__` is an object, not a function, and `{}` SURVIVES serialization.**
`typeof obj['__proto__']` is `"object"`, and
`JSON.stringify({side: obj['__proto__'], id: 1})` is `{"side":{},"id":1}`. So for
that one name the field is **present and empty**, not absent. The function-omission
reasoning covers the eleven callables only. `__proto__` requires its own native
witness and separately priced cause; it must not be buried under callable omission.

An unruled object-output loss starts as `FINDING`. After examining the actual
producer and consumers, the owner may explicitly choose the same closed-field
policy for the prototype object as for callables: retaining a closed shared enum
rather than adding an inherited-object variant. That ruling is `CHOSEN` under
[`divergence-taxonomy`](#divergence-taxonomy), not a repaired mapper or a parity
claim. It must state whether Rust refuses the whole result or omits an optional
field, and retain the separate `{}` wire witness. The previous unconditional
"its own FINDING" wording conflated discovering this loss with deciding to keep
it; a representation decision does not erase the loss or change its status to
`ported`. Branch, interpolation and evaluation-order defects remain defects.

### Open-String fields must carry the INTERPOLATED value

The exclusion below is load-bearing and has a measured shape. Where the shared
field is a `String`, TypeScript interpolates the inherited member into it, and the
result is **not the key name**:

- an inherited callable becomes `function <ITS OWN NAME>() { [native code] }`.
  It is **member-specific**, not a constant: `constructor` gives
  `function Object() { [native code] }` — the only one that differs from its key
  — while `toString` gives `function toString() { [native code] }`, and
  `valueOf`, `hasOwnProperty`, `isPrototypeOf`, `propertyIsEnumerable`,
  `toLocaleString` and all four `__{define,lookup}{Getter,Setter}__` likewise
  carry their own names. So bitget's `tran_id` reads
  `bill:function Object() { [native code] }` for `constructor` and a *different*
  string for every other callable;
- `__proto__` becomes `[object Object]`.

The shared coercion must therefore retain **`ProtoMember` identity** through to
the string, rather than emitting one canned value.

Those strings are representable in Rust and therefore **must** be reproduced.
This is `js_string` applied to an inherited member, and it belongs in the shared
primitive that already owns JS string coercion — never as a venue-local literal.

### What the row must still carry

Citing this is not a licence to skip the witness. Each row keeps:

- a `ts-bug:` marker, since the lookup defect is TypeScript's;
- a witness asserting `typeof` **before** JSON serialization erases the function
  — after serialization the field is merely absent, which is indistinguishable
  from a dozen other causes, and this construction has been the only thing able
  to see a defect four separate times in this project;
- the **in-memory** divergence stated explicitly: TypeScript's consumer reads a
  truthy function where Rust's reads a refusal. **The serialized forms do not
  converge either** — see the section above.

### What this declaration does NOT cover

- **The branch decision.** If a `||`, `??` or `!` takes the wrong path, that is a
  `TableHit` predicate used incorrectly, not this boundary. Fix it.
- **An open-string field.** Where the shared field is already a `String`, the
  wire value *can* be carried and must be — this covers closed enums only.
- **A venue widening its own local type.** Still forbidden.

### An inherited hit can cause a WRITE to a different record

The lookup class is usually described as a misread: `TABLE[wireKey]` returning a
truthy function for an `Object.prototype` name, which suppresses a `||` or `??`
fallback and inverts a `!` guard. Bitget measured a worse consequence on a real
store path. A ticker symbol of `constructor`, `toString` or `__proto__` resolves
the inherited value, whose `id` is `undefined`; `updateTicker` then stringifies
that and **updates a REAL ticker whose id is the string `"undefined"`**, if one
exists — a measured state change plus its notification, on a row the caller never
named.

So the class is not bounded by "a value is read wrongly". Where an inherited hit
feeds a key used for a subsequent WRITE, the blast radius is another record, and
a row's note should say which of the two it reproduces. The shared `TableHit`
branch behaviour is what preserves the distinction; a venue-local prototype map
would erase it.

### WHEN the refusal happens is part of the boundary

A typed-output refusal must not PRE-EMPT a later error the source would have
reported. Measured twice on one venue, at two unrelated sites: in a batch
reduce, an inherited row followed by a null row must surface the LATER NULL
error, because the source keeps going and reports that one — while an eager
per-row refusal surfaces the inherited-member refusal instead and the test still
looks like it passed, since *an* error was reported.

The repair is a DEFERRED refusal flag evaluated after all rows, not a widened
model: the boundary still refuses, it just refuses at the point the source would
have finished. Same shape at a third site, where an early inherited-market
refusal masked source numeric coercion errors and the fix was to evaluate every
numeric field before the typed refusal.

**And the consequence is not only WHICH error — it is observable SIDE-EFFECT
COUNTS.** Measured on an algo sibling: with `[inherited child, null]` the source
returns a function-valued order and **one** clock; with `[inherited child, 1]` it
throws a later in-operator TypeError before mapping, with **zero** clocks. Rust
eagerly refused the first child in both, collapsing two different source
outcomes into one. The same shape reaches the network on a placement path, where
an eager refusal can skip HTTP requests the source would have issued — so on any
row that dispatches, assert the REQUEST COUNT as well as the error.

**An oracle that compares values and errors but NOT side-effect calls cannot see
an evaluation-order defect.** A 1178-case history oracle missed one: with a
throwing `uTime`, the source calls `toBase` for amount and filled BEFORE the
TypeError, while the Rust mapper parsed `uTime` first and made ZERO calls. Same
error, same output, two different source effects. The oracle compared
single-field errors and never callback invocations, so every one of those 1178
cases agreed. Count the calls, not only the result.

So a row that cites this declaration owes more than "the inherited value cannot
enter the typed field" — it owes WHEN. A witness that only asserts an error
occurred cannot tell these apart; assert WHICH error, and on a batch, assert it
after the batch.

<a id="owned-snapshot-vs-live-alias"></a>

## 16. Owned snapshot versus live alias

Canonical representation declaration: **`owned-snapshot-vs-live-alias`**.
**Cite it. Do not restate it per row.**

A row whose **entire** remaining divergence is that TypeScript hands back a LIVE
REFERENCE where Rust hands back an OWNED SNAPSHOT cites
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`. A MIXED row cites
it *alongside* its own causes.

### The fact being cited

A JavaScript object is a reference. When a function stores or returns one, the
caller and the callee hold the **same** object, so a mutation made through either
is observable through the other, **after** the call.

Rust's owned values — `serde_json::Value`, a cloned struct, a snapshot read — are
copies taken at one instant. Every later mutation of the source is invisible
through them, and every earlier snapshot stays frozen.

Two independent instances, which is why this is a declaration:

- **`Order.info`** (coinbase). Executed TypeScript proves
  `ready.value.info === gateway` by reference identity, so a later relabel of the
  gateway *and* a later `gateway.price = NaN` are both observable through `info`,
  while `order.price` keeps its earlier snapshot.
- **`BaseExchange.constructor`** (core). The source retains the caller's options
  BY REFERENCE, so a retained alias observes later patches. Rust shares one
  internal owner but returns owned snapshots, so a snapshot taken before a patch
  stays unchanged.

### Why this is `CHOSEN` and never `FORCED`

Rust **could** alias: `Rc<RefCell<T>>`, or any shared interior-mutable handle,
reproduces it.

We decline, and for a concrete reason rather than taste. Aliasing pushes interior
mutability across the whole typed model, and it converts a stale-value bug into a
**panic** — a `RefCell` double borrow aborts, and wasm cannot `catch_unwind`, so
where TypeScript would merely read a stale field Rust would take down the module.
A borrow held across an `await` is the live form of that hazard and has already
compiled cleanly once in this project before failing at runtime.

**That is the why-mirroring-would-be-worse sentence for all of these rows.**

### What the row must still carry

- Which direction is observable: a mutation visible through the alias, a snapshot
  that stays frozen, or both.
- A witness recording **both views** — the source's and ours — from a live-source
  oracle, not a fixture.
- Any SEPARATE loss on the same field. `Order.info` has a separate native-NaN
  domain cost; core's options snapshot makes no such claim. Distinguish loss
  BEFORE the mapper input from a mapper that discards an admitted numeric value.
  `serde_json::json!(NaN)` already produces null, so passing that value to a mapper
  does not prove a mapper-induced collapse. Do not let an alias citation absorb it.

### An explicit JSON input choice has a separate native-number price

Selecting `serde_json::Value` for shared `Order.info` and gateway inputs is
`CHOSEN`, not a claim that Rust cannot hold native NaN. A dynamic numeric object
domain could carry NaN without mutable aliases; retaining the one JSON
representation used by transport, Store and consumers avoids a second numeric
value representation across the existing Value-consuming ABI, with different
coercion and serialization behavior. The same Value-input choice excludes own
undefined properties, executable coercion hooks, functions, symbols, BigInt and
custom prototypes. Retaining one object contract for existing Value consumers
avoids embedding another JavaScript object runtime in each producer. A row must
identify its actual producer boundary and observed refusal or omission cost;
this citation is not proof of arbitrary-input conformance.

A ruling must price that a TypeScript caller can supply
and later inspect native NaN while the Rust mapper cannot receive that value
through its selected input type. Witness the source before serialization and
the Rust input before invoking the mapper; neither aliasing nor a successful
null-to-null map establishes numeric parity. This choice does not excuse loss
of an admitted JSON number: signed zero, overflow and underflow on typed numeric
paths remain repairs under [`numeric-ingress-defect`](#numeric-ingress-defect).

### What this declaration does NOT cover

- **A value that cannot be represented at all** — that is
  [`shared-utf8-string-boundary`](#shared-utf8-string-boundary).
- **A value that cannot cross a closed typed field** — that is
  [`closed-typed-output-boundary`](#closed-typed-output-boundary).
- **A shared owner that is genuinely shared.** Where Rust *does* hold one internal
  owner and the divergence is only in what READS return, say so: the state is not
  duplicated, only the view is.

<a id="numeric-ingress-defect"></a>

## 17. The numeric ingress defect — REPAIRED, and each owner flips their own rows

**The shared operation LANDED on 2026-10-13**, merged into the integration branch
at `02689776f9` from `e5f0069ae1`. This section is still not a licence to cite: a
row does not become `ported` because the repair exists, it becomes `ported` when
**that row's own witness flips on the landed operation** and its owner says so.
Until then it stays `divergent` with its `FINDING`.

### What landed, and the one thing to read before using it

```
safe_cex_num::numeric_ingress::{
    js_from_str, js_from_value, js_from_value_ref,
    JsNumberInput, JsNumberOutput, js_value_as_f64,
}
```

Registration goes through the `js_number_output!` **expression** macro inside an
explicit `JsNumberOutput::check_number_output` impl — never a macro that declares
the impl, because an item-declaring macro is invisible to the invention gate.
Generic payloads consume `Envelope<JsNumberInput>` and then `data.decode` /
`decode_with`; `Value` has **no `JsNumberOutput` implementation at all**, so the
lossy path does not compile rather than failing at runtime.

**Existing production decoders are untouched.** The operation is OPT-IN per
owner. There are **TWO** regression guards over shipped code, not three: binance's 9
fields across 4 records and hyperliquid's `OrderType.trigger.triggerPx`. Both
were measured clean **for SIGNED ZERO**, and that is the half that must not
change; their owners are re-measuring an opt-in by bits, not hunting a
regression.

**grvt's `bd`/`qd`/`fi` is NOT a guard** — `GrvtInstrumentLite` is still pending
with no committed Rust declaration, and its clean result came from the
pre-Value-first map-only CANDIDATE that was never shipped. Its owner is
preserving that measurement as a regression FIXTURE for when the type is ported.
A measurement taken on a candidate is evidence about the candidate; calling it a
guard over production was this section's error.

**"Regression guard" does not mean "nothing changes."** binance's own correction:
the same four records that are clean on signed zero still carry an existing RAW
OVERFLOW REFUSAL — four of its six divergent rows are those `FINDING`s — so
opting in SHOULD turn `raw 1e400` from a refusal into `Infinity` while the
signed-zero bits stay put. A guard is a guard for the shape it was measured on.
Measure both shapes before reclassifying any row. **Existing raw `Value` behaviour is NOT
globally declared unsafe**; the output graph needs the shared buffer because the
typed adapter itself can introduce a native `Infinity`.

**Dependency delta is zero** packages, versions and features: `raw_value` rides
`safe-cex-num`'s own dependency line, and the workspace `serde_json` entry — the
one `how.rs` pins byte-identical against `infra/tradetunnel` — was not touched.

Evidence behind the landing: Node 24 and 26, 2019 spellings with 50475 bit and
path comparisons each; four pinned compiler refusals plus a positive payload
fixture; three compiling production mutations failing named precision, depth and
output-guard assertions and then restored; host and wasm strict clippy
separately; four blockers found and closed by its own adversarial review before
any landing claim.

Twelve agents measured this independently across roughly **250 numeric fields**,
on every venue that audited. It is the largest single defect found in this port.

### What has flipped, and what cannot flip yet

**First confirmed flips, 2026-10-13, binance at `9d7f0ef7d7`** after a reviewer
MERGE and compiled controls: four raw-overflow `FINDING`s became `ported` —
futures `GET_POSITION_RISK_RESPONSE` (`updateTime`, `adl`, `adlQuantile`), PM
`GET_POSITION_RISK_RESPONSE` (`updateTime`, `adlQuantile`),
`BinanceIncomeRecord.time` and `BinanceIncomeHistoryOptions`
(`startTime`, `endTime`, `limit`). `raw ±1e400` is accepted as `±Infinity` and
the signed-zero bits did not move, which is the shape split this section
predicted. Measured over 162 source cases and 1134 bit comparisons across seven
NAMED routes (raw str, raw slice, owned `Value`, borrowed `Value`, `js_from_str`,
`js_from_value`, `js_from_value_ref`) — a flip measured on one route proves
almost nothing. Bybit then repaired 21 numeric paths including the hand-written
`TickSize` `as_f64`, which closes **mechanism 6**, and asterdex's six flattened
fields exercise **mechanism 5**.

**There are TWO adoption depths, and a flip claim must say which one it used.**

*Registration plus shared entries* leaves the type's own `Deserialize` alone: the
shared entries are repaired and a bare `serde_json::from_str::<T>` still
reproduces the old behaviour. *Rewriting the public `Deserialize` body* to capture
`JsNumberInput` and hand off to a private remote companion through
`strict_decode::object` repairs the bare path too — woo did this and its 91-field
witness exercises bare `from_str` and `from_value` alongside the shared raw,
owned and borrowed APIs, so it makes no helper-only claim. Both are legitimate;
they are not equivalent, and the row note must distinguish them.

**Under the shallower depth, the operation repairs the ENTRY PATHS only.** kucoin
measured this and deliberately did NOT move its counts: its four projections pass
224 bit comparisons from 72 source cases through the shared raw/owned/borrowed
and native entries, while its **public bare-serde entry points still reproduce 36
refusals and 4 sign losses**. Registration does not rewrite
`serde_json::from_str::<T>`. So a row flips only when its PRODUCTION CALLERS go
through the shared entries — and where no production decode caller exists yet,
the honest note distinguishes the repaired supported API from the legacy bare
boundary rather than claiming `Deserialize` itself was fixed. Any owner reporting
a flip should be able to say which caller it measured.

**Eight shared graphs are still INELIGIBLE, and they include the core domain
models.** From the shared-model census: `Balance`, `Market`, `Order`,
`Position`, `Ticker`, `ExchangeOptions`,
`ExchangeOptionsExternalPublicData` and its `Variant1`. They carry `Value` info
fields or host capability types, and the owner declined to declare them
passthroughs silently. **The named passthrough clause has since landed** and the
eight are adoptable through it; what follows describes why it was needed. It is the
clause that captures
`JsNumberInput` before the `Value` is constructed, keeps the public `Option<Value>`
shape, and names the field and the escape in the row. Treat an opt-in on a record
that embeds one of those eight as blocked, not as done.

**A passthrough is an ELIGIBILITY exception, not an output-check skip.** The
owner who first consumed it measured this: shared `decode_with` still runs
`check_numbers` RECURSIVELY over the whole output, `raw_response` included. So the
clause says "this field may be a `Value`", not "stop checking here" — a
`Value::deserialize` substitution inside it compiles and then fails by name at
*numeric ingress forbids Value buffers*. Describing it as a hole in the guard —
as this section's author did in orchestration — is wrong, and the distinction is
the whole reason the design was acceptable.

The orphan rule does NOT block a venue whose record merely embeds a shared type:
implement nothing for the foreign type, and instead exhaustively destructure the
embedded shared struct inside your OWN `check_number_output`. Asterdex proved it
on six flattened fields. The exhaustive destructuring then stops compiling when
the shared type gains a field, which is the forcing function — a new number
cannot arrive unchecked.

### Why it survived every gate

**`-0 == +0` in Rust and in JavaScript.** Every value-based comparison in this
project passes straight through the sign loss. Nobody wrote a bad test; the tests
could not see it. The two refusal shapes below look like correct strictness until
you check what Node actually accepts.

**Compare f64 BITS, not values.** `8000000000000000` against
`0000000000000000`. A witness that compares values is vacuous here.

### Three shapes

**Shape 1 — the sign is lost through a rebuffer.** An already-parsed `Value`
carrying an integer lexeme `-0` enters with bits `8000000000000000` and leaves as
`0000000000000000`.

**It is the integer SPELLING that is lost, not the value, and the rebuffer is
what loses it.** Measured on mexc's envelope with the fields held constant and
only the spelling varied, bits asserted before any JSON output: decoding the same
field directly from RAW retains both signs, while a `from_value` rebuffer turns
integer-lexeme `-0` into `+0` and leaves float-lexeme `-0.0` negative. Native
`Value(-0.0)` also keeps its bits. That strongly indicates serde_json's
integer-versus-float `Number` representation — an integer-spelled `-0` has no sign
left to recover once it is an integer — but **the internal cause is a hypothesis
with strong evidence, not a proven mechanism**, and the owner who measured it
says so. The consequence for a consumer is the part that is settled: a field that
only ever receives `-0.0` is safe where the same field receiving `-0` is not, and
the Rust type cannot tell you which will arrive.

**Independently confirmed on a second venue**, and it cost a clean result to find:
coinbase had reported all 13 of its numeric fields clean and no rows needing
reclassification, then its own deeper review separated an in-memory `Value(-0.0)`
from a PARSED `Value` holding the integer token `-0`. Raw typed `from_str`
preserves the sign and an in-memory float `Value` through `from_value` preserves
it, while parsed `-0` → `serde_json::Value` → `from_value` loses it. Three
enclosing type rows went back to `divergent` and the ported count fell from 249
to 246. That is the seventh owner to revise a ported count DOWNWARD after a deeper
audit, and the first to do it after a clean result had already been praised.

**Shape 2 — raw overflow is REFUSED where `Value` paths accept it.** `raw ±1e400`
fails with *number out of range*; a parsed `Value` retains `±Infinity`. Node
yields `Infinity` for both. Negative **underflow** behaves the same way, and is
the one case where shapes 1 and 2 meet.

**Shape 3 — valid numeric spellings are refused.** Plain decimals (`1.5`,
`1.25`), exponents (`1e3`), signed zero. Node accepts all of them. Worst on
tagged, untagged and flattened paths, one of which is integer-only today.

### Seven mechanisms — a fix must satisfy ALL of them

(Mechanism 7 has since collapsed into mechanism 1; the entry is kept with its
correction because the wrong conclusion is instructive.)

1. **Lexeme-to-f64 rebuffer** — the base case, via `from_value`.
2. **A NESTED second buffer, which loses the sign EVEN FROM RAW.** This breaks
   the intuition that raw paths are safe. Measured on `BybitRateLimitRow`.
3. **Serde internally-tagged enums** — refuse `raw -0`, `-0.0`, `1.5`, `1e3` with
   *invalid type: map, expected f64*. This implicates tagged-enum rebuffering as
   a mechanism SEPARATE from record ingress: a record-only fix leaves every
   tagged enum broken.
4. **Untagged unions** — lose more than their sibling fields in the same record.
5. **Flattened structs** — also reject plain `1.25`.
6. **Explicit hand-written `as_f64`** — not a serde path at all, so a serde-only
   fix misses it. Grep for `as_f64` on any numeric ingress path.
7. **Generic containers parameterised over `Value`** — ~~lose the sign inside
   `Value::deserialize` with no outer rebuffer~~. **CORRECTED: the container is
   innocent and this is not a seventh cause.** Lighter measured a container over
   `Value` losing the sign where the same container over a typed parameter stayed
   clean. **The generalisation to "the `Value` parameter is the problem" was this
   section's, not that owner's** — lighter's own rows scoped the loss to the
   owned/borrowed rebuffer and pinned direct raw as clean from the start. mexc then measured
   the same shape — `MexcEnvelope<Value>` — CLEAN when decoded directly from raw,
   lossy only when an already-parsed `Value` is rebuffered through it. So
   mechanism 7 collapses into mechanism 1: one cause, the rebuffer. A fix aimed
   specifically at generic containers would be aimed at the wrong thing, and
   whole-entry decoders cover both.

### The mechanism that works

**Retain the raw LEXEME under `arbitrary_precision`. Do not rebuffer through
`Value`.** That approach is already proven twice in production in this tree:
overflow retention in the HOW decoders, and finite negative zero in
`normalize_json_numbers`.

Two measured constraints on the design:

- **A leaf helper cannot fix `Value(-0)` through `deserialize_any`.** Whole-path
  preservation is required before any leaf can claim to repair an
  upstream-erased sign.
- **Native `Infinity` is projected to `null` by an intermediate
  `serde_json::Value` visitor.** That is the same visitor behind shape 7, and the
  eighth instance of the non-finite-to-null family in this port.

An explicit `Value` `as_f64` helper must **parse the existing numeric lexeme and
never synthesize NaN JSON** — JSON cannot hold NaN, and synthesizing one emits
invalid JSON.

### Acceptance set

This is a repair requirement, not a declaration accepting numeric loss. The
shared typed-f64 ingress must preserve binary64 bits through raw JSON, owned and
borrowed Value inputs, nested record buffers and tagged/untagged union paths.
Its maintained corpus includes decimal and exponent forms, positive and negative
zero, positive and negative overflow, and **negative underflow** such as
`-1e-4000`. JavaScript accepts that valid JSON number and produces negative zero
(bits `8000000000000000`). An overflow fix and a signed-zero fix tested separately
do not establish that their intersection, negative underflow, works.

WOO's `tests/numeric_ingress.rs` records the distinguishing existing behavior:
85 scalar/optional fields preserve raw negative zero, overflow and underflow but
lose an already-parsed Value's integer lexeme `-0`; seven untagged instrument
number|string fields additionally lose raw `-0` and reject overflow and negative
underflow. Native `Value(-0.0)` and lexical `-0.0` are separate positive controls.
These are FINDING witnesses that must flip when repaired, not desired behavior.
Observe the typed f64 bits before JSON serialization: numeric equality hides the
zero sign, and JSON serialization hides both negative zero and non-finites.

Use the measured field lists, and require a **nested record decoded from RAW**
and at least one **internally-tagged enum** — a fix validated only on top-level
raw or on records alone will pass and leave those paths broken.

TWO sets were measured CLEAN in shipped code and lost the sign ONLY in a reverted
`Value`-first candidate. They are the **regression guard**, proving the fix does
not introduce the defect where it does not already exist — not repairs: binance's
nine fields and hyperliquid's `triggerPx`. Both were clean **for signed zero
only**; see the shape note above, since binance's same four records still carried
the raw overflow refusal and those four rows did flip.

**grvt's three `GrvtInstrumentLite` fields are NOT in that set** — the type has no
committed Rust declaration, so its clean result describes the reverted candidate
rather than shipped code, and its owner keeps it as a FIXTURE for when the type is
ported. This paragraph said otherwise until 2026-10-13 and contradicted the
correction above it; the contradiction was found by the owner reading both.

### A process rule this produced

**A CLEAN RESULT NEEDS ITS SCOPE STATED.** Two agents reported clean, correctly,
having audited only their newest slice — and then found the defect in their
OLDER decoders. If a row or report says clean, it must say what was audited.

<a id="divergence-taxonomy"></a>

## 18. Every `divergent` row carries a BUCKET — and how to audit that claim

### The bucket requirement

This rule was issued and enforced through orchestration for most of this port's
life and was never written down, which is why it is here now. Rules that live
only in messages do not survive.

**Every `divergent` row's note must open with exactly one of five buckets:**

| Bucket | Meaning |
| --- | --- |
| `FORCED` | Rust **could not express** the source behaviour. Rare — verify before using it. |
| `CHOSEN` | Rust **could** mirror it and we decline. **Requires a why-mirroring-would-be-worse sentence.** |
| `MIXED` | Both, stated separately. Cites a shared declaration *alongside* its own causes, never instead of them. |
| `CORRECTED` | Rust deliberately does not reproduce a source defect. Must say so explicitly. |
| `FINDING` | An open defect with an owner. Becomes `ported` when the fix lands and the row's witness flips. |

`FORCED` is the one most often claimed wrongly. Three rows on one venue called
BTreeMap reordering FORCED when an insertion-ordered map mirrors it exactly, and
a whole class of rows called a representation refusal FORCED when the project had
simply chosen `String`. If Rust can express it, the bucket is `CHOSEN` and it owes
a reason.

**When one cause of a multi-cause row is repaired, the BUCKET MOVES.** `FINDING`
means an open defect with an owner. If a row was `FINDING` for the numeric
ingress defect and `CHOSEN` for something else — bitget's five `requestTime`
fields are the first case: the raw-overflow half closed on the landed operation
while a UTF-8 string-boundary residual keeps the row divergent — then the note
must stop saying `FINDING` and become `CHOSEN` or `MIXED` with the remaining
cause named. Leaving the old bucket in place makes a closed defect look like
open work, and the count of open `FINDING`s is the thing anyone reads to decide
what is left.

Also required, and not implied by the bucket: a `ts-bug:` or `ref-bug:` marker
where a source defect is preserved, and a witness. **Where a value is observable
only before serialization, the witness must assert `typeof` or f64 BITS BEFORE
`JSON.stringify`** — that construction was the only thing able to see a defect
five separate times in this port, because a serialized comparison converges while
the in-memory values differ.

**Thirteen divergent rows do not currently carry one of the five keywords**, in
`safeCexAxiosCreate.ts`, `exchanges/external-public-data.ts`, `utils/risk.ts`,
`utils/virtualClock.ts`, `utils/roeRoi/calculateReturns.ts`,
`instrument-class/types.ts`, `instrument-class/bybit.instrumentClass.ts` and
`store/store.base.ts`. Several convey the same thing in other words
("Intentional divergence", "Independent …", a bare `ts-bug:`). They are not wrong,
they are unclassifiable by a gate. Normalise them when next in the file.

### CORRECTED — the invention gate DOES resolve symbols, and this entry was wrong

This section previously reported a blind spot: that the gate checked `rs` files
and not `rs` symbols, evidenced by `utils/get-key.ts`'s `s` and `c` naming
`safe-cex-num/src/get_key.rs::s` and `::c`, symbols that "nobody ever wrote".

**Both halves were false, and the gate itself proved it.** `get_key.rs` carries
`pub use crate::lodash_snake_case as s;` and `pub use crate::lodash_camel_case as c;`
— the symbols exist, at exactly the paths the rows name, and the original rows
were correct. The claim came from a definition-only regex
(`pub fn|pub struct|pub enum`) that cannot see a re-export alias. Repointing the
two rows at `lodash_case.rs` immediately produced FOUR gate errors: two `status:`
errors saying the renamed target needs a note naming that exact Rust target, and
two `invention:` errors saying `get_key.rs::s`/`::c` were now public items with no
`rs` mapping. The rows were reverted.

So the gate resolves the leaf symbol in both directions already — a row naming a
symbol that does not exist fails, and a public symbol no row names fails. **Do not
build a second symbol-existence checker.** It also means a `pub use X as y`
re-export is a first-class parity target: an alias is how a row keeps the
source's own name when the shared implementation lives elsewhere, and that is the
pattern to use rather than inventing a note explaining a rename.

### A macro that DECLARES items is invisible to the invention gate

The gate resolves `rs` targets and public items from the source it can see, and
an item-generating macro defeats that: the items exist after expansion and the
indexer never sees them. **So a registration mechanism must be an exhaustive
EXPRESSION macro inside an explicit `impl`, not a macro that declares the impl**
— which is the form the numeric-ingress operation was rewritten into once parity
refused its first shape.

This is the gate working, not failing: it declined to index declarations it
cannot read rather than passing them silently. Treat it as a design constraint
on anything that generates public surface, and prefer the explicit impl even
where a declarative macro would be shorter — the shorter form buys brevity by
removing the surface from the only inventory that checks it. The same reasoning
settled the generated collation region, where the whole data module is PRIVATE
and so its 149 consts are not public surface at all.

### What a clean audit result must state

**AN AUDIT OF ALREADY-PORTED ROWS IS A REACHABILITY QUESTION, NOT A CALL-SITE
GREP.** When a shared seam is found, the rows that cross it are not only the ones
that name it. Measured across the fleet on 2026-10-13, after the Store
notification contract turned out to be infallible in Rust where TypeScript
handles a subscriber throw: a direct scan of ported rows returned zero for six
venues; one venue then traced three of its ported send helpers reaching the
notification INDIRECTLY — no-socket reconnect to processClose to health
publication to Store notify — and reclassified them, which no call-site scan can
see; re-run as reachability, roughly a dozen rows across five venues had been
asserting a parity they did not have, and one venue's "no set_setting" zero was
simply wrong.

So an audit answer must state **where the follow-through terminates** (local
arithmetic, a cached-map read, a built-in) and **which direction each edge runs**
— an INCOMING caller cannot make a ported row cross a seam. Caller-supplied
closures are out of scope by declaration, or the walk never terminates. A zero
that answers the narrower question is worse than no answer, because it closes
the question.

**A limit imposed by a repair has its own cost, and it is a CHOSEN divergence.**
The shared numeric ingress was required to keep serde's recursion limit on every
path, so that the documented depth boundary could not become conditional on which
decoder a caller took. Measured consequence: Node and the compiled legacy bare
decoder both accept a 140-deep message array that the shared capture now refuses.
Three worker rows close their numeric `FINDING` and stay `divergent` as `CHOSEN`
for that narrowing. Record the cost; do not let a repair quietly buy correctness
with acceptance.


**A CLEAN RESULT NEEDS ITS SCOPE STATED.** Two agents reported an audit clean,
correctly, having examined only their newest slice, and then found the same defect
in their older decoders. And when auditing the manifest with a script, validate
the instrument first: a regex over notes reported 122 dead declaration anchors
here, and all 122 were the regex capturing a trailing sentence period. The real
count was zero. The corrected section above is the SECOND instance in one audit:
a definition-only regex reported two nonexistent symbols that the gate resolves
fine as re-exports. The pattern in both is the same — a hand-written regex over
annotations, trusted over the gate that already measures the same thing. **Run
the gate first; if it passes and your script disagrees, your script is the
defect.**

<a id="diagnostic-prototype-write-choice"></a>

### Diagnostic stats: prototype-write scope

The earlier CHOSEN no-write policy applies only to adapters that deliberately
retain typed own records without reproducing inherited referent mutations. It
is not a Rust limitation. BaseWebSocket now captures a PrototypeRealm and
reproduces builtin/scalar alias writes, later enumeration and cleanup within
that configured record boundary. The old blanket no-write rationale no longer
describes those methods. The anchor remains for historical citations.

Bounded evidence is in safe-cex-venue-binance/PROTOTYPE-GATES.md: 1280 statistics
cases through case1279 and 20 authored sequences through case19. The separate
shared-core source-fed comparison has 78 profiles through
ordinary/ownHandlers=2/ownStats=true; it is not an independent native-first census.
Arbitrary executable getters remain outside the configured record boundary.

The callback-count projection finding at
safe-cex-exchange/src/websocket.rs::BaseWebSocket::handler_stats_count is repaired.
Native scalar counts now match the source -0.5/true witness and the later
reconciliation/cleanup errors. Bounded native-first evidence, exact Number bits,
JSON projections and named native/source corruption controls are recorded in
safe-cex-venue-binance/SCALAR-LENGTH-GATES.md. The new matrix stops at iteration
119 (Spot public, empty String, own record); six additional native sticky-true
cases stop at iteration 5. This closes the recorded scalar projection gap within
the configured boundary, without widening executable-getter support.

<a id="okx-market-record-choice"></a>

### OKX market helper input choices

Both helpers select materialized market records from the native response
pipeline, rather than requiring a live JavaScript property/call host for
otherwise pure record transformations. Mirroring that host is feasible. The
CHOSEN cost excludes callable, accessor and prototype-mutated receivers and
their read-time effects. In particular, the source currency helper can read a
ticker's id or symbol more than once; its native TickerMarkInput stores a
single materialized value for each field.

`findTickerMarkForCurrency` retains the shared numeric capture's bounded
decoder instead of introducing an unbounded recursive parser for ignored
fields. The bound keeps capture within the same stack/resource profile as
other native JSON ingress. Its cost is whole-input refusal when an ignored
field exceeds that profile, even though the source helper never reads it.
This is CHOSEN; it does not describe the separately witnessed string cost
governed by [§19](#forced-surrogate-conflict).

`OKXExchange#formatMarketPrettySymbol` retains own JSON field reads, coercion
order and cold-branch behavior within that materialized input profile.
Its returned string observation is separately governed by
[§19](#forced-surrogate-conflict).

<a id="owned-exchange-lease-policy"></a>

### Owned exchange leases and imported observers

The Binance concrete owner uses strong child/parent leases and weak socket
registrations back into the core parent. Class async invocation jobs and externally retained
child owners keep their receiver alive. Quiescent roots, failed construction and
settled jobs release their owned graph without requiring explicit disposal.
An isolated ExchangeState view is not an owner lease and does not keep child
owners alive; its socket registry may consequently contain no live entries.
A retained child leases its receiver state, rather than the outer root's sibling
owners. Operations requiring those siblings retain the ExchangeOwner root.
Externally retained HTTP interceptor handles also do not lease a released native
exchange: invoking one after the last owner lease returns `exchange owner released`.
The maintained source interceptor retains its JavaScript receiver and still runs
after the caller clears its owner binding. This lifetime difference is CHOSEN:
deterministic native ownership bounds retained host resources without introducing
a tracing collector or keeping an arbitrary imported client alive through an
uncollectable Rc cycle. It does not authorize weakening active jobs or child
leases. Bounded native/source witnesses and controls are recorded in
safe-cex-venue-binance/CLASS-CLOSURE-GATES.md.

<a id="store-typed-patch-choice"></a>

### Store patch model ownership

DefaultStore keeps one authoritative typed StoreData graph for venue, risk and
worker consumers. Preserving those typed consumers while accepting arbitrary replacement types
or an auxiliary bag of unknown fields would require another model which those
consumers cannot read consistently. The native patch ABI therefore omits unknown fields and refuses
wrong-shaped known fields, including null for required fields. Validation of a
candidate model or batch precedes commit, so rejection is atomic; source direct
assignment can expose earlier writes before a later throwing property. The
sealed PatchSource also excludes executable property access and its side effects.
These are CHOSEN restrictions, with ownership governed by [§16](#owned-snapshot-vs-live-alias).
They do not authorize changing an admitted info JSON object or numeric value.

<a id="cross-trade-native-capabilities"></a>

### CrossTrade capability ownership

The native manager and order factory use one object-safe exchange/runtime
capability set. A second dynamic Exchange object/receiver protocol would split
account lookup, placement and scheduler ownership from the native interfaces
which actually drive them. Rc retains supplied callable and result identity;
Result carries native failures. Arbitrary JavaScript receiver properties and
throw values are not carried through that selected protocol. This is CHOSEN;
concrete facade, Store and browser scheduler assembly is a separate requirement,
not established merely by supplying the traits.

Config, copier and market/store inputs follow [§16](#owned-snapshot-vs-live-alias)
so sizing and execution consume the same owned typed records instead of a
parallel mutable heap. Caller changes after capture, reentrant getter side
effects, shared frozen-default identity and later nested-alias mutations are
not reproduced. Explicit native enum/object admission and the bounded shared
numeric decoder exclude malformed fields and over-depth input. Stock inherited
symbol identities must still take the source order-selection branches; the
shared TableHit adapter now carries those identities instead of inventing strings.
Custom or mutated prototypes, callable execution and a live object heap remain
outside that adapter. See [§14](#shared-utf8-string-boundary),
[§17](#numeric-ingress-defect) and [§19](#forced-surrogate-conflict).

Native placement/audit prefixes start eagerly. The returned future owns the
continuation, avoiding a second implicit executor and cancellation owner beside
the runtime scheduler. Dropping or rejecting an aggregate can cancel siblings;
source Promise siblings can continue. Weak loop captures avoid a scheduler
callback keeping a stopped manager alive indefinitely, at the cost of source
retained-callback lifetime. These are explicit scheduling choices, not exact
Promise execution parity.

<a id="cross-trade-diagnostic-choice"></a>

### CrossTrade advisory diagnostics

CrossTradeRuntime exposes advisory formatted diagnostics separately from its
fallible placement capabilities. Making raw variadic console objects, inspection
getters and console throws part of that protocol would give the diagnostic host
a second implicit order-control/error channel. We choose one native message and
infallible diagnostic methods instead; source inspection identity, formatting
and thrown-console effects on subsequent execution are lost. The muting message
uses the numeric deadline used by the native clock rather than a source ISO
rendering. That spelling is a priced diagnostic choice, not time-format parity.

<a id="transport-relay-policy-scope"></a>

### Native relay policy scope

The native relay enable flag is deliberately one process-wide policy. An atomic
cell keeps thread callers on that one policy rather than independent thread-local
copies. Independently instantiated source module realms can retain separate
flags; matching that isolation would require explicitly carried realm-owned gate
handles throughout native callers. We choose the process scope and lose
per-realm/tenant isolation. This does not claim a worker flag binding or prove
that every native consumer is wired to the gate.

<a id="weex-fill-projection-choice"></a>

### WEEX fill input and model choices

The execution and historical-trade adapters share the retained numeric/JSON
`WeexUtilityInput` boundary instead of a second executable object/coercion host.
Callable properties, accessors, custom prototypes, Symbol and BigInt are outside
that selected profile. Mirroring them is feasible but would add a separate
runtime beside the shared ingress used by the other venue utilities. Ordinary
JSON coercion errors remain inside the profile and must retain source order.

Whole-record capture keeps the shared decoder's recursion/resource bound and
retains unknown fields before either function can take a cold branch. A selective,
branch-dependent parser could admit more inputs; we choose one ingress policy
for the utility family. The CHOSEN cost includes whole-input refusal for deep
ignored fields and for ignored/unobserved lone units, losing an otherwise normal
record or null result. It is separate from the returned-unit observations governed
by [§19](#forced-surrogate-conflict).
`safe-cex-venue-weex/tests/fills_error_order.rs` witnesses both
forms against the actual source, including the cold ignored-unit case.

The shared Execution and HistoricalTrade models keep optional boolean maker/buyer
fields and a string commission asset. Widening them would change the consumer
contract across venues, so these adapters choose the same closed model rather
than a parallel dynamic result. Present null/nonboolean flags and nonstring
assets can therefore lose the whole source result, without coercion or silent
omission. Admission belongs after the source guards and coercions: it must not
replace a later source conversion failure, skip the second historical timestamp,
or turn the source's missing-order null result into a model error. See
[§15](#closed-typed-output-boundary) and [§18](#divergence-taxonomy).


Final strict-field decision (claude-ddi2il, 2026-10-17): constructed WEEX
`fetchHistoricalFills` must retain the good rows of a page. Its one shared
normalization pipeline distinguishes source failures from closed output-field
projection refusals. For a maker or commission-asset refusal, the concrete
binding emits the existing warning with the logical field and rejected value,
then omits only that row through the existing truthy filter. An all-refused page
resolves empty after reporting each refusal. It never coerces a flag, substitutes
an asset, or stringifies a value into a model field. This is an accepted
divergence: the maintained TS mapper retains those malformed fields and rows.

The public single-row utility continues to return its original TypeError for
that refusal. Capture/coercion failures remain separate, preserving the source
scalar evaluation order, and a warning callback failure propagates normally.
The actual constructed Exchange regression covers mixed pages, raw coin
fallback, absent/true/false flags, all-refused pages and source commission-error
precedence; the preexisting native-first `fills_error_order` source oracles stay
unchanged. No shared Execution/HistoricalTrade type or transport API is widened.

<a id="weex-record-shape-choice"></a>

### WEEX shared Market and response record choices

`normalizeMarket` retains the shared required-string Market result instead of
introducing a parallel dynamic utility result. The CHOSEN cost is refusal of
source results whose directly assigned symbol/baseAsset/quoteAsset fields are
missing, null or nonstring. A second result shape would split the shared consumer
contract. All source scalar work must finish before this admission; this choice
does not authorize replacing a later coercion failure with a model error.

`normalizeTicker` and `fakePositionsForMarket` retain a complete typed Market
input rather than a utility-specific projection of only the properties read.
Keeping the same DTO across native callers avoids a second partial input model.
They adopt the materialized input domain of
[§16](#owned-snapshot-vs-live-alias), including its executable-object distinction.
Their full Market can be refused for unread fields before invocation, even when
the source algorithm returns normally. This is input admission, not a mapper
discarding an admitted native Market. Per-row observations identify the lost
result and the fields actually tested.

That Market ingress, and the public JsNumberInput codecs for
WeexExchangeInfoResponse and WeexOrderResult, adopt only the shared whole-record
capture/resource/ignored-field policy at
[weex-fill-projection-choice](#weex-fill-projection-choice), not its fill-specific
output-field paragraph. The response codecs represent declared shapes; they do
not establish Axios runtime validation. Separate string and snapshot observations
remain governed by [§19](#forced-surrogate-conflict) and
[§16](#owned-snapshot-vs-live-alias).

Final strict-field policy for the constructed WEEX market batch: a row whose
required string field cannot inhabit `Market` is refused alone, with a visible
field/value warning, retaining good rows and returning empty when all are refused.
`normalize_market_row` completes source coercions before this projection; genuine
source/capture failures retain the original outer catch and cached-market result.
The public utility retains its standalone refusal. The maintained `fetchMarkets`
and `normalizeMarket` source probe keeps the raw malformed fields in twelve mixed
pages, so this row loss is explicitly CHOSEN. Keeping shared model fields strict
avoids introducing malformed identities/assets into every downstream consumer;
per-row refusal avoids discarding the valid snapshot. Constructed Exchange HTTP
regressions independently pin native IDs/assets, warning values, empty-page
semantics and source coercion precedence. No source-equivalent row claim is made.


<a id="weex-class-assembly-choice"></a>

### WEEX explicit class assembly

The three WEEX executable class aggregates require complete external member
adapters for their heterogeneous native interfaces. They retain the existing
host capability contracts rather than add another type-erasure bridge that
implicitly couples every constructor to every concrete transport, Store and
callback ABI. The cost is that constructing a class needs a supplied adapter
table, unlike the maintained TypeScript class with fixed member bodies. Missing
or mismatched entries refuse construction before superclass effects. Matching
target labels prove table provenance, not concrete member-body linkage.

`classes.rs` implements own-arrow binding and public prototype disposal through
one opaque superclass receiver. Receiver lifetime, superclass behavior and
concrete adapter bodies remain required host capabilities. The recording host
in `tests/classes.rs` establishes those binding operations, not a complete
production JavaScript/transport/Store assembly.

<a id="weex-async-observation-choice"></a>

### WEEX reporting and continuation ownership

`fetchOrders` discards rejected normal/algo legs without reporting, matching the
maintained Promise.allSettled body. Its constructed HTTP regression observes
successful orders and zero error events after an algo endpoint rejection.
Public depth-gap recovery silently discards snapshot rejections and routed-callback
throws, matching the maintained empty catch. The initial listenOrderBook snapshot
retains its distinct source warning. Constructed HTTP/socket coverage observes
no gap warning, no failed replacement delivery and a subsequent valid update.
The three private refresh callbacks silently catch fetch and Store-update failures,
matching source. A constructed account frame with a rejected REST balance refresh
retains the old balance without reporting a host exception.

The returned `fetchOrders` future owns its aggregation/dedup continuation;
refresh futures own update continuations. Dropping one can omit those outcomes,
even when host-started
requests survive. This bounds abandoned native work instead of requiring every
borrowed host context to survive an unobserved Promise. Public snapshot observers
are different: `observe_snapshot` must retain its callbacks and receiver until
settlement, including success-callback throws. That is a required host contract,
not a cancellation proof from a recording fixture.

The imported `settled` capability supplies Promise.allSettled reaction semantics
and peer retention. Refresh reactions suspend after a ready fetch result before
checking disposal or applying it. These boundaries do not implement an
entire JavaScript microtask scheduler. Shared pager ownership and callback costs
remain at [exchange-runtime-callback-choice](#exchange-runtime-callback-choice)
and [shared-handler-and-ledger-choice](#shared-handler-and-ledger-choice).

<a id="forced-surrogate-conflict"></a>

## 19. Surrogate observability resolves the §14 conflict

The discriminator is **observability after the boundary**, not the fact that
Rust strings are UTF-8. This ruling replaces §14's former unconditional
“CHOSEN and never FORCED” statement and its unconditional substitution ban.
Keep the existing `forced-surrogate-conflict` anchor for older citations; the
conflict itself is resolved by the rules below.

- **FORCED:** name the exact materialized lone UTF-16 unit and the actual
  program observation: comparison, hashing, key lookup, transmission, or a
  returned value a caller can inspect. Also name the required Rust string
  surface that cannot retain it. This is not a claim that Rust globally lacks
  UTF-16 representations. A whole-record refusal is a larger loss than losing
  one field and must be stated explicitly.
- **CHOSEN and priced:** when the only consequence is human diagnostic text,
  U+FFFD substitution is permitted as a declared cost. State that the reader
  loses the original retained surrogate half and its exact escaped/forensic
  spelling. A log/error/debug label alone does not prove this scope: trace the
  actual consumer. If a program reads the unit, this exception does not apply.
- **PORTED ASCII:** a literal backslash-u sequence that stays uninterpreted is
  ordinary ASCII and must remain byte-exact. Valid ASCII JSON wire text that
  `JSON.parse` turns into a lone code unit is **not** this case. Observe the
  returned value before serialization; escaped witness transport does not
  turn a materialized unit back into an ASCII-only source value.

### Applications and exact observations

`accounts/parseImportAccountData.ts::parseImportAccountData` is **MIXED**.
The actual importer returns an account whose unknown value/key or name holds
one UTF-16 unit (`typeof === "string"`, length 1, units D800 or DC00); an array
import returns both accounts, including the offending value. Rust refuses the
**whole import**, with no data, because its `Value::String`/object-key strings
cannot retain the observed unit. That half is FORCED. The separate 128-level
serde recursion budget stays CHOSEN: the source accepts a 130-deep unknown
field while Rust refuses it. The existing `Invalid JSON format` diagnostic is
recorded as the Rust outcome, never as a claim that these inputs are invalid
JSON. `safe-cex-types/tests/import_observability{.rs,_oracle.cjs}` is the later
actual-method supplement: ten independently pinned outcomes, eleven returned
views, and four native positives distinguish scalar Unicode, paired units,
U+FFFD, and literal six-character backslash-u text. Original oracle bytes are
unchanged.

`exchanges/base.ws.ts::BaseWebSocket#capWsDiagnosticPayload` has a measured
**programmatic consumer exception**, not an assumed human-only consequence.
With request logging enabled, the actual `grafana-pws` callback stores the
message in `requestsLogAtom`; the actual dev/Cypress `__getRequestsLog()`
returns it unchanged. At the split boundary the returned string has length
4121 and unit D83D at index 4095. The helper is available in dev/Cypress, not
plain production without Cypress. The source consumer proof is
tealstreet-next `scripts/sdk-diagnostic-observability.test.cjs`, which executes the
actual cap, callback and getter with real Jotai storage before serialization. It
lives in the monorepo because its consumer is apps/web, outside this slice.
The cap row's FORCED clause must name this observation; do not classify it as
CHOSEN merely because the event is called diagnostic. Its core owner retains
responsibility for the native cap witness and row note.

`http-over-websocket/axiosHttpOverWebsocketAdapter.ts::summarizeTradeTunnelUrl`
is the **diagnostic-only candidate**: its fallback summary is consumed by the
request/response verbose log functions and `console.log`. If the owned
consumer audit establishes no programmatic unit reader, replace its old
FORCED clause with CHOSEN and price the lost surrogate half at the 500-unit
split. A measured programmatic consumer would instead use the FORCED rule.
This declaration does not invent a completed consumer audit or silently edit
another owner's row.

Lighter's flatten/parser witnesses and AsterDEX's stream witness are **not
ASCII-only cases**. Lighter measured returned `id`, `bids[0]` and `symbol`
strings containing materialized units. AsterDEX's actual `handleStreamData`
forwards `data.s` to `handleTickerUpdate` with type string, length 1 and unit
D800; its labelled supplement was published at `a97bbd525b`. Owners retain
separate per-row causes and move their notes using these exact observations.
Lighter's rejected reference-price `Error.message` still requires its own
consumer audit; an unsuccessful literal-classifier search is not proof of
human-only use. No row is promoted to PORTED merely because its JSON witness
contains an escape.


<a id="kraken-utility-wire-domain"></a>

## Kraken utility wire input domain

`CHOSEN`: Kraken fills/account-log utility adapters consume ordinary JSON data,
plus an explicit missing scalar value, rather than arbitrary executable JavaScript
objects, Symbol or BigInt. Callable coercion hooks, custom prototypes and getters
are outside this input adapter. Mirroring them would require a second executable
object model at a venue JSON boundary and move host execution into parsing, while
transport and downstream shared records still exchange data. The source's explicit
`Number(BigInt)` and number-hint hook results remain measured in the live utility
oracle as accepted TypeScript inputs with no corresponding Rust wire input.

The string restriction cites [shared-utf8-string-boundary](#shared-utf8-string-boundary).
Whole mapper inputs use shared `JsNumberInput` capture and its 128-level recursion
budget, including ignored fields. Mirroring unrestricted nested input here would
bypass the shared parser budget and create a second ingestion policy in this venue.
The live supplement runs the actual TypeScript mappers with a 140-deep ignored
field and escaped lone surrogates, captures their outputs, and asserts Rust
representation/depth refusal for those same raw bytes. These are accepted source
inputs, not invalid JSON. No replacement characters or silent dropping are used.

Within that domain each source function is mapped separately. Shared numeric
capture precedes untagged buffering; a private wire enum registers every variant
and sends native numeric values through the shared retained-JSON construction for
coercion. No numeric or string coercion is reimplemented. Local Date.parse times
require an explicit host wall-time-to-UTC adapter; tests measure UTC and UTC+02.
Resolver callbacks are fallible and their call order and errors are compared.


<a id="kraken-module-utility-adapters"></a>

## Kraken module utility adapters

`CHOSEN`: `parseTriggerSignal` uses the JSON input domain declared in
[kraken-utility-wire-domain](#kraken-utility-wire-domain); its native enum is
observed before serialization. `sort` and API-error messages use the
[shared UTF-8 boundary](#shared-utf8-string-boundary), with actual-source
surrogate acceptance and Rust representation-refusal witnesses.

`sort` composes the shared pinned root collator. The measured source profile is
Node's en-US default collator (the oracle fixes and asserts that host locale),
with Node, V8 and ICU versions recorded. Its actual numeric comparator results
are compared directly with Rust Ordering, without a sign-normalizing oracle.
A caller-selected host locale is outside this profile: mirroring ambient locale
changes would make signing-key order depend on host configuration instead of the
shared collation contract. Source operands remain strings; coercing arbitrary
JavaScript receiver objects is outside this adapter.

`createKrakenApiError` uses shared ExchangeError. It retains absent versus null
response/cause data, with cause stored in `metadata.original_error`. JSON response
and cause values become snapshots, while TypeScript retains live aliases. Rust
Display emits the message alone; TypeScript Error stringification prefixes its
name. The witness measures both representations and an alias mutation after
construction. JavaScript stack/prototype/descriptors and executable error inputs
are not carried across this host error ABI. Mirroring them in a venue-local error
class would bypass the shared HTTP/exchange error plumbing and merely move the
loss to its next conversion. Message, name and distinct metadata fields are
asserted directly on native Rust values, not serialized roundtrips.

`createKrakenSpotClientOrderId` exposes a generator factory with host-supplied
fallible CSPRNG filling. Create it once per SDK runtime/module and share it among
all Kraken Spot instances, matching the source module-owned pool; recreating it
per order or per venue is not equivalent. Its closure implements the actual
nanoid 6.0.1 optimized hexadecimal pool path. The default 32-character size is the
only supported call shape; TypeScript's optional manual size override is measured
but deliberately omitted, because exposing invalid-length venue client IDs would
add an API used by no Kraken call site and bypass the venue's fixed-length rule.
The host entropy source is explicit to serve native and wasm without importing a
Node/browser global. The oracle runs the actual dependency, pins its version and
source checksum, injects entropy only at crypto.getRandomValues, and compares ID
bytes, entropy draw sizes, growth, failure state and capped refills. It does not
claim to test a production host's CSPRNG or instantiate the pending exchange class.


### Kraken parsed read boundary

`CHOSEN`: Kraken `_fetchAccountLogPage`, Spot `fetchWsToken`, and Spot
`fetchPositions` expose their method bodies through shared Store and explicit
read callbacks. This slice does not instantiate either exchange or implement
Axios factories, signing, retry or transport assembly; those are separate pending
rows. The callback receives the actual endpoint plus query/body, and returns the
parsed Axios `data` (None represents undefined), or the original boxed host error.
Read invocation occurs at function entry, before polling, and errors propagate
without conversion or loss of identity. Account-log symbol lookup happens after
the read resolves against the then-current Store, matching the actual arrow body.
Spot positions contain no await and return their computed native vector directly
instead of wrapping it in an already-fulfilled JavaScript Promise.

Rust futures require host polling and can be dropped; JavaScript async methods
schedule their continuation independently of whether the caller retains the
Promise. No detached task is created by these body adapters. Pending exchange
assembly must explicitly own and poll the future, and must not claim Promise
lifecycle/cancellation parity from these row witnesses. A JSON-shaped data
snapshot replaces live JavaScript objects, getters and prototype customization.
The supported domain, UTF-8/depth refusal and explicit date-host adapter are the
same as [kraken-utility-wire-domain](#kraken-utility-wire-domain); the live corpus
measures a source lone-surrogate token and an ignored deep field that Rust refuses.
Executable objects and mutation through aliases are outside this owned boundary.
The numeric query input requires the source's declared two number fields; actual
TypeScript forwards string-valued fields too, which the Rust adapter deliberately
rejects rather than widening a typed query contract. A paired witness measures
that difference. Distinct since/count native values and a symmetric Serde rename
control check the decoded fields before any output serialization.

Spot positions consume the shared typed Store's Market domain, rather than an
arbitrary JavaScript object masquerading as a Market. Native fields, enums,
active-market filtering and long/short order are asserted directly. The source
itself emits constant positive zeros in five separate position fields; no input
can distinguish a swap among those equal constants. Per-field constant mutations
therefore exercise every numeric output, while distinct symbols/ids and the
nonzero leverage detect other swaps. The read token remains an open JSON native
value because the actual source returns any truthy token, despite its string
annotation. The witness compares native numeric bits (NaN only canonicalized),
not a serialize/decode roundtrip. Capture headers include architecture and Node
major. All cases execute the actual TypeScript in one Node process per corpus.


### Kraken leverage cache host

`KrakenExchange.fetchLeverageSettings` uses the same owned parsed-data and explicit
future boundary as [kraken-parsed-read-boundary](#kraken-parsed-read-boundary).
The free function accepts the originating exchange's read, cache clear/set, and
console-error operations. It does not create a venue-local Store or cache: the
pending constructor must retain the exchange-owned cache and supply these methods.
The host owns/polls the future. A synchronous read failure reports before return;
cache writes occur only after the awaited read succeeds.

CHOSEN: callback failures are shared `ExchangeError` values with UTF-8 String
messages, rather than arbitrary JavaScript thrown primitives or getter-bearing
objects. Console arguments preserve the fixed label and typed error message.
Owned JSON tree values replace live object aliases. Distinct object/array keys
from distinct JSON inputs remain distinct keys in the witness; equal primitive
keys overwrite without changing insertion order. This does not implement a shared
JavaScript Map engine or promise cache integration in the pending constructor.
The shared numeric ingress captures all number-bearing input before untagged
buffering; no integer narrowings are introduced for symbol or maxLeverage.

Named ts-bug compatibility: `fetchLeverageSettings` catches read/cache failures,
reports them with console.error, and resolves. A cleared cache or earlier set
remains committed after a later set fails. A console failure still propagates,
including its shared subscriber metadata and metadata allocation. No notifying
Store write is introduced or discarded. String-valued preference collections
iterate stock characters without entries; other truthy non-iterables clear before
reporting their TypeError. Null preference entries preserve their source failure.

The 33-case actual-source supplement witnesses result guards, clear/set order,
truthiness, duplicate primitive keys, distinct equal-shaped object/array keys,
synchronous and asynchronous read failures, cache failures, console failures,
partial effects, nonfinite numbers, negative zero, and lone-surrogate refusal.
The original read oracle stays byte-identical; one Node invocation executes the
original corpus and both supplements. Source totals/per-case operation and cache
counts are pinned independently, with 33 completed native case comparisons.


### Kraken journal delegation

`KrakenExchange.fetchHistoricalPositions` delegates to the landed shared
`fold_closed_position_records` with `symbol_scoped = false`. The host supplies
its originating shared Exchange and JournalFolder; no Kraken folder is introduced.
The actual-source witness executes both the unchanged venue arrow and base arrow,
with the imported folding engine represented by a recording dependency. It does
not claim the pending Kraken constructor or fills route is integrated.

CHOSEN: options enter via shared JsNumberInput into an owned UTF-8 account string
and optional f64 bounds. Omitted/null bounds use the shared defaulting rule;
negative zero is retained. Arbitrary runtime non-string accounts and object
getters are outside this typed ABI. A paired witness proves the source forwards
a numeric account while this adapter refuses it. The host polls the Rust future;
this does not implement JavaScript Promise scheduling or pre-poll execution.
The shared operation owns capability rejection, clock defaults and settle lookup.
Returned shared record Values and ExchangeError are passed through without
serialization, cloning, reshaping, or flattening subscriber identity.


### Kraken Spot candle host

`KrakenSpotExchange.fetchOHLCV` uses the owned parsed-data boundary from
[kraken-parsed-read-boundary](#kraken-parsed-read-boundary). Shared JsNumberInput
owns options/response numeric ingress, Store supplies the market view, and the
host polls the future. The request callback receives the endpoint and native
pair/interval/since components; shared TableHit preserves inherited interval
identity without pretending a function is JSON. The pending API factory must
assemble these components into the actual Axios-shaped request. Numeric bounds
are f64; source truthiness omits zero/NaN from, and floor divides milliseconds.

CHOSEN: the options ABI is UTF-8 symbol/interval and optional numeric from,
without getter/prototype mutation. The Candle output profile requires
numeric timestamps. Source strings and nulls are retained
by TypeScript despite its nominal Candle type; paired witnesses demonstrate that
the typed profile reports a diagnostic and returns empty instead. This explicit
refusal prevents an implicit string-to-number conversion from masquerading as
parity. Price/volume parseFloat behavior still preserves nonfinite values and
signed zero, observed on native fields with only NaN capture canonicalized.
Timestamp ordering uses stable sorting with source subtraction semantics (NaN
comparison means equal), preserving equal-time rows and numeric infinities.

The console callback has the fixed source label and shared ExchangeError message
ABI, as in [kraken-leverage-cache-host](#kraken-leverage-cache-host). Read, decode
and mapping errors are caught; successful console reporting resolves to empty
(ts-bug compatibility), while a console error propagates its original allocation
and subscriber metadata. No Store write or error-emitter call is added.


### Kraken Futures ticker host

`KrakenExchange.fetchTickers` uses the owned parsed-data boundary from
[kraken-parsed-read-boundary](#kraken-parsed-read-boundary). Shared numeric ingress,
get-key lookup, JavaScript coercion and shared Ticker outputs retain the mapping;
the host polls the future and supplies the parsed request and fallible trace.
Market lookup occurs after the request resolves. Pending factory wiring remains
pending. No Store notification or venue-local pager is introduced.

CHOSEN: the typed Store excludes inherited Object-prototype pseudo-markets. A
paired witness executes the source constructor-key pseudo-market while native
returns no ticker. The fallback is an owned snapshot, whereas TypeScript returns
the Store array by reference; both witnesses observe that ownership difference.
Getter/prototype mutation and arbitrary thrown JavaScript values remain outside
the typed host ABI. Errors use the shared ExchangeError cause/metadata contract.

Named ts-bug compatibility: request or mapping errors are traced and swallowed,
then the current Store tickers are returned. Trace may change the Store before
that read; trace failures propagate with original metadata allocation and ordered
subscriber IDs. The actual-source supplement pins 49 cases, 63 returned records
and 693 direct native numeric comparisons, with distinct fields and fixed bounds.
Only NaN is canonicalized; signed zero remains bit-exact.

<a id="cryptocom-signing-data-domain"></a>

## Cryptocom signing data domain

`CHOSEN`: `paramsToString` accepts ordinary JSON data with stock prototypes and
an explicit top-level missing value. Nested undefined, holes, Symbol, BigInt,
callables, getters, custom prototypes and cycles are outside this adapter.
Mirroring these would introduce an executable object model into signing data
while transport and shared records exchange JSON. The live source witness
observes representative excluded inputs separately from Rust comparisons.
Strings follow [shared-utf8-string-boundary](#shared-utf8-string-boundary).
Witness and incoming-adapter capture use shared numeric ingress with its recursion
budget; that budget is not an outgoing recursion limit of `paramsToString`.
Numeric lexemes are coerced through shared JavaScript String conversion.


### Kraken account validation UTF-16

FORCED: Futures `validateAccount` indexes `error[0]`. A valid UTF-8 astral
string can therefore produce an observable lone-surrogate error in TypeScript.
The shared `ValidateAccountResult` / `AccountValidationError` JSON string model
cannot retain that value. The paired oracle observes source UTF-16 units without
attempting a lossy JSON-string decode. Native validation reports the representation
cause through its normal outer catch for a response envelope, or propagates a
TypeError when the error originates inside the catch. Neither path silently emits
an altered source string as if it were parity.

Captured response fields have no getters. Numeric HTTP statuses 401, 403, 429 and
400 return their source diagnostics even when `response.data.error` is astral;
no UTF-8 conversion of the unused temporary error unit is required. Spot joins
whole error strings and creates no corresponding string-index representation gap.
HTTP/signing factories remain pending; these mappings cover the two method bodies.

<a id="cryptocom-signing-execution-boundary"></a>

## Cryptocom signing execution boundary

Signature and auth helpers execute synchronously at invocation through the shared
local HMAC backend. Request and WS builders read the host VirtualClock before
traversal/signing. Coercion TypeErrors preserve name and message. No remote signer
is configured. Returned method/key strings are owned values.

Authenticated requests capture SigningValue before clock callbacks: primitives
copy and object/array nodes retain shared identity. Owned property/element reads
and short mutation operations expose no guards. Rebinding leaves previous roots
and children alive. Cycle-forming edits are rejected; repeated references remain
valid. The existing ordinary-data scope remains at
[signing data](#cryptocom-signing-data-domain), with its separate observed string
causes under [surrogates](#forced-surrogate-conflict).

The graph serializer follows the actual source object-depth cutoff, UTF-16 key
order and immediate coercion/error order directly. It does not expand a whole
DAG before signing. No user callback runs under a graph lock. Host parallel-write
scheduling and arbitrary executable JS data are not established by this ABI.
These builders do not establish HTTP serialization or socket-dispatch parity.

<a id="cryptocom-helper-value-boundaries"></a>

## Cryptocom helper value boundaries

The module public-endpoint binding refers to one retained native array per thread,
including sparse slots and UTF-16 string entries. Clone retains identity; content
replacement mutates the same allocation. Classification reads that actual array
with stock Array.some/String.includes semantics, with no escaped borrow guard.
Indexed accessors, prototype-indexed properties, overridden builtin methods and
non-string elements require the language host and are not certified by these
concrete typed operations. The generic runtime interceptor's host-global helper
is a separate wiring boundary, not an assembled-runtime claim.

Both classifiers have lossless UTF-16 input entries and ergonomic UTF-8 wrappers.
The private-write classifier retains the source's nine captured endpoint values.
The interval mapper returns mapped strings or actual shared PrototypeRealm
referents; its lossless entry also returns UTF-16 error messages. Existing shared
builtin implementation boundaries remain; no whole JS object/function runtime
or complete cross-host scheduling implementation is inferred.

<a id="cryptocom-dto-json-snapshot-boundary"></a>

## Cryptocom DTO decoded-JSON snapshot boundary

The eleven wire DTO owners capture ordinary decoded JSON before private typed
validation. Unknown properties, duplicate-key replacement slots and JavaScript
integer-index ordering are retained, recursively through nested record and extra
objects. Numeric event capture precedes private typed decoding. The assembled-output
numeric guard runs after decoding and extra-field attachment; opaque extras are
explicitly identified numeric passthrough fields, not a waiver of known-field
numeric validation. Numeric output uses native
IEEE values before JSON projection. Captured property order and current typed
fields drive serialization together; original known-field values are not replayed.

This is an owned snapshot plus in-place field projection ABI. It does not retain
source object aliases, getters, prototypes or arbitrary public-field deletion and
reinsertion chronology. New field names have deterministic native append order;
that is not proof of arbitrary JavaScript mutation history. Typed known fields
remain authoritative over colliding extra names, including absent optional fields.
The finite witnesses cover shared raw, owned-Value and borrowed-Value entries and
plain Serde raw decoding. Other Serde entries and generic REST/WS decoder adoption
are not established. The separately observed string cause remains under
[§14](#shared-utf8-string-boundary) and [§19](#forced-surrogate-conflict), so this
repair alone earns no ported status. In each of these eleven owners, the source
JSON.parse probe retains the observed D800 string field while the native
String/Value::String capture boundary refuses the whole record, including its
otherwise usable fields. Row notes identify the observed field paths. Ownership
and output claims remain subject to
[§15](#closed-typed-output-boundary), [§16](#owned-snapshot-vs-live-alias),
[§17](#numeric-ingress-defect) and [§18](#divergence-taxonomy).

### Kraken Futures candle host

`KrakenExchange.fetchOHLCV` uses the
[parsed read boundary](#kraken-parsed-read-boundary), with shared numeric ingress
and coercion. CHOSEN: options have UTF-8 symbol/interval and optional f64 bounds
and limit; testnet is a bool. Arbitrary option getters and JavaScript thrown
values are outside this typed ABI. The host polls the future, supplies the clock,
parsed HTTP response, fallible console reporting and fallible error emission.
The pending factory must assemble native URL/from/to components; no response
parser phase is inferred. No polling timer is implemented by this read body.

Response field coercion, including string/null timestamps, uses shared JS number
and parseFloat semantics. Inherited interval keys preserve stringification and
NaN range arithmetic. Sort remains stable and compares timestamp differences.
CHOSEN: mixed NaN/non-NaN timestamp collections are refused, because the source
comparator is inconsistent and Rust requires a consistent ordering. A paired
actual-source witness records V8 accepting `[3, 2, NaN, 1]` and ordering it as
`[1, 2, 3, NaN]`; native refusal reports/emits a named TypeError and resolves
empty. All-NaN collections retain source order.

Named ts-bug compatibility: missing candle data is diagnosed before an error is
raised; the outer catch reports that error, emits its message or fallback text,
and resolves empty. Failure of the missing-data diagnostic enters the catch;
failure of the catch diagnostic or emitter propagates without flattening native
error metadata. The actual-source supplement pins 47 cases, 74 records and 444
direct numeric field comparisons. Capture records CPU/Node; only NaN is canonical.


### Kraken Spot balance host

`KrakenSpotExchange.fetchBalance` uses the [parsed read boundary](#kraken-parsed-read-boundary)
with shared numeric ingress, shared asset normalization and shared Store/Balance
models. CHOSEN: the host supplies a fallible POST callback (the factory must send
an empty object body) and a fallible error emitter. The callback's parsed result
retains own-key ordering, raw asset/balance info, alias overwrite and NaN behavior.
Ticker lookup happens after the response resolves; the first USD/USDT match wins.
Yield/reward/tokenized sub-balances containing a dot are skipped.

CHOSEN: an error resolves to an owned snapshot of the current Store balance after
emission; source object aliasing is unavailable through the shared owned ABI.
Emitter failure propagates with original metadata. String result enumeration
uses UTF-16 units within the shared UTF-8 domain; a surrogate pair requires lone
surrogate values that this ABI refuses, with a paired actual-source witness.
No Store write or account factory is implemented by this body. The witness pins
29 cases and 49 native currency comparisons, plus the paired string boundary.

### Kraken Futures order read

`KrakenExchange.fetchOrders` uses the existing parsed-read boundary with shared
numeric ingress, String/parseFloat/date coercion, trigger parsing, Order and
Store. The host supplies a fallible GET, current clock, local-wall-time to UTC
conversion, and fallible console trace. Continuations read the current Store;
console callback failures preserve original ExchangeError metadata identity.
The native body supplies the exact OPEN_ORDERS endpoint; the factory remains
pending and must implement the GET itself. The reference witness pins UTC.

CHOSEN: on error the native body returns an owned current Store order snapshot;
the source returns the Store's live array. The paired witness mutates the source
returned order and observes the Store change, while native mutation remains
isolated. Shared Order optional fields also collapse explicit undefined trigger
slots to None: source stop orders with an unrecognized trigger own
slTriggerType: undefined, while native None has no separate own-slot presence.
The actual-source witness records that presence and native None directly.
Retained order info is owned prepared data rather than an arbitrary JS alias
or getter graph; existing UTF-8/retained-value ingress boundaries still apply.

No order write, Store write, concrete exchange factory, or streaming protocol is
implemented by this read body. Stream adapter rows only construct/connect and
forward through host-bound socket callbacks with the declared shared arguments;
the concrete WebSocket classes and their event processing remain separate rows.
The Futures public WebSocket's OHLC handler and OHLC subscription method remain
the source's intentional no-ops; they do not implement the REST candle poller.

### Kraken startup, snapshot and idle-feed hosts

The two exchange start bodies use host-bound initialization, data fetch, Store
update, socket construction/connection and fire-and-forget tick operations.
The host represents the original exchange receiver and supplies the existing
shared StoreDataLoaded flags. Fetched batches stay opaque Rc handles: these
bodies neither decode nor clone their payloads. StartupUpdate forwards the
original batch and the source's exact flag merge to the Store binding; it is an
operation envelope, not a second Store model. Concrete factories, protocol
classes, tick scheduling and individual Store dependency behavior remain their
own rows. Host futures carry continuation settlement timing; the body does not
implement a JavaScript microtask scheduler. The host tick operation must report
its unawaited asynchronous failures independently, matching the existing shared
runtime contract; startup does not await its completion.

Spot fetchAndSetOrders similarly returns the original opaque fetched handle.
It calls the existing full-order Store binding, then reads current loaded flags
and marks orders loaded, preserving listener-induced flag and batch changes.
This port does not add a second Store implementation or claim to repair alias
limits in its underlying dependencies.

The idle-feed helper ABI supplies a lazy source for-in enumeration of the
lengths of declared handler arrays, including inherited enumerable entries.
A missing/null handler has absent length; other lengths are nonnegative array
lengths. Enumeration/length getter failures arrive as the original ExchangeError
at the corresponding iterator step. The helper stops before later getters once
an active array appears. The dictionary/prototype enumerator belongs to the
pending socket class/host binding. Feed arguments are opaque borrowed values;
UTF-16 topics are forwarded without UTF-8 coercion. No feed recording, handler
registration or timers are inferred from the helper alone.

### Kraken lifecycle hosts

The four Kraken connectAndSubscribe bodies in lifecycle.rs retain the initial
disposal guard, network-specific URL, create/assign sequence and four listeners
in source order. ConnectionHost supplies the pending concrete factory and
original open/message callbacks; an opaque Any event passes through unchanged.
Each listener registration reads the current slot, including replacement during
an earlier registration. Error callbacks read the current slot again before
readyState and close, catch failures from that source try block, and always call
the captured close continuation. The continuation invokes the host's current
onClose operation. Callback/factory errors outside that try propagate unchanged.
The error path retains source CLOSED=3 and its exact guard; it adds no reconnect
policy, subscription payload, scheduler or complete websocket class claim.

KrakenExchange.dispose is a host-bound body over lazy live Map-value and Map-key
iterators. The host provides the existing Map storage, timer cancellation, set
and clear operations, and the superclass dispose binding. The body cancels every
yielded original timer handle, marks every yielded key inactive, clears timeouts,
active flags and candles in that order, then calls the superclass operation.
Reentrant deletion skips an unvisited entry and appending extends the iterator,
as source Map iteration does. UTF-16 keys and opaque timer handles retain identity.
This is not a replacement Store, timer engine, exchange class or JS Map runtime;
those dependencies remain separately mapped. Native tests assert the operation
and retained-state contracts against actual source bodies, with fixed finite
floors of 25 connection cases per family and 11 disposal cases. No full-class
or unlimited event-loop enumeration is implied by these method-row ports.

### Kraken public subscription and Spot dispatch hosts

Both public onOpen bodies are mapped through PublicSubscriptionHost. The host
binds existing product selection/market symbol snapshots, Object.keys on the
current handler maps, current handler-array lengths, and JSON.stringify/send.
Ticker snapshots are fresh local arrays, as the source map/filter helpers produce.
Keys are snapshotted only when each source loop begins; lengths are read lazily
from the current map before each subscription. Tickers use groups of 50; Futures
book errors invoke the exact console-error operation and continue, while Spot
subscription errors propagate. Futures still reads OHLCV handler lengths without
sending an OHLCV frame. No disposal recheck is inserted after callbacks.

PublicSubscription retains every literal payload field and UTF-16 topic in a
native operation envelope. Spot key splitting uses the first two :: components,
and the existing Spot INTERVAL table retains mapped numbers, inherited function/object
identities and the fallback. JSON.stringify/send is a bound dependency, including
omission of inherited function values and serialization of lone surrogates; no
new serializer or wire-format parity claim is made by these body rows. Witnesses
inspect actual source payloads before stringify and compare native fields before
any wire operation. Recorded source wires are supplementary, not native evidence.

Spot handleBook and handleBalances bind message.data plus Array.isArray through
SpotDispatchHost.array_data. None denotes a non-array; a lazy iterator denotes
the original array, including live length changes and opaque original items.
Symbol getters/truthiness, each current message.type read, child handlers and
parent refreshBalance are bound dependencies. Book dispatch forwards original Rc
items and symbols without coercion. Balance refresh is initiated synchronously
and not awaited; its host must visibly report any asynchronous failure. The
concrete classes, JSON getters/array iterator and child parsers remain separately
mapped. Fixed source-native floors are 43 subscription cases per family, 14 book
dispatch cases and 6 balance dispatch cases, not full runtime enumeration.

### Kraken periodic synchronization and Spot cancellation hosts

The two tick bodies use TickHost for bound fetch methods, current Store operations,
Promise task ownership and loop scheduling. Every twentieth count, including zero
and negative multiples, fetches all three account snapshots; Futures first awaits
leverage settings. Each then callback preserves its original Rc batch, applies
the full snapshot operation before reading current loaded flags, and sets only
its corresponding flag. Falsy balances do not update Store. Count arithmetic is
native f64, including signed zero, NaN and infinities; omitted counts default to
zero and scheduling uses 500 ms.
The captured loop callback calls the host's current tick method with count + 1,
including replacement after the callback was registered. Disposal is checked
at entry and before scheduling, not after individual data awaits.

start_then must register an owned task without executing its callback during the
array-construction stack. promise_all observes every child and completes at the
first rejection without cancelling siblings; task ownership and observation must
continue after the enclosing tick completes. The host supplies future settlement
and microtask timing, exact error-message-or-fallback emission, superclass Store
semantics and the existing loop helper. These rows do not claim a new Promise
scheduler, polling engine, Store or complete exchange class. Witnesses use the
actual source bodies, independently controlled settlement, immediate fulfillment,
late sibling updates, original batch identity and loaded-flag reentrancy.

Spot cancelOrders binds the original order array, length/index/getter operations,
its live array iterator and unlimitedXHR.post. Empty arrays issue no request;
single orders issue one request and return, while multiple orders await each
POST before retrieving the next current ID. The existing CANCEL_ORDER endpoint
and txid field are retained, with opaque original IDs including UTF-16 strings.
Responses are ignored as in source. The error-message-or-fallback binding and
emitAndLogError run before a fresh shared ExchangeError is returned. Exception
representation remains the shared runtime ABI; no venue-local exception model
or serializer is introduced. Host bindings retain source getter/error timing.

### Kraken numeric stream update hosts

stream_updates.rs maps both public handleTicker bodies and Spot handleOHLC through
opaque values with bound property access, actual get-key lookup, parseFloat and
Date/string operations. These intrinsics and current handler/ticker maps remain
dependencies; this is not a new JS value runtime, parser, Store or websocket class.
The ticker methods retain original current ticker handles and emit ordered native
numeric patch entries. Undefined/null omit fields; NaN, signed zero and infinities
remain present numeric values. Futures aliases are lazy nullish fallbacks, relative
funding is read without absolute funding substitution, and percentage calculation
requires a positive open plus a present last. Spot derives mark/index from last
and retains volume/change fields. Property/get-key reads keep source order.

Spot OHLC reads symbol and interval before the truthiness guard, finds the first
strict numeric match in the existing Spot INTERVAL table, then builds a UTF-16 key.
Alias intervals therefore select the earliest source timeframe, not the requested
subscription's preferred alias. Missing handlers skip stats and construction.
Stats precede Date conversion and numeric reads; timestamp divides milliseconds
by 1000. The shared Candle starts without lastSide. Rc<RefCell<Candle>> preserves
the one candle object across callbacks, including first-callback mutation seen
by later callbacks. The bound array forEach captures its length at invocation,
reads current callbacks and skips deleted entries; the captured handler array
remains original even if the handler dictionary changes. Errors propagate.

Native witnesses execute actual venue bodies and the actual get-key declaration,
use installed lodash helpers and shared Rust numeric/date helpers, then assert
native patch presence/order/fields, target identity, candle fields, UTF-16 keys,
callback mutation and original errors before any wire operation. Date fixtures
run in explicit UTC; production date bindings must implement the host timezone.
Finite floors are 31 cases per ticker family and 45 OHLC cases. No unlimited
enumeration, native serializer parity or full intrinsic implementation is claimed.

### Kraken websocket message and private control hosts

`message_dispatch.rs` ports four `onMessage` bodies. The declared MessageHost
binds JavaScript values and direct/optional property access, truthiness, strict
string comparison, String/template coercion, optional includes, JSON.parse and
source jsonParse, transport diagnostics, current receiver state and method
calls. Native parent-log calls carry explicit LogSeverity::Error. `parse_data` preserves string versus optional-toString/falsy fallback
selection (`{}` for the three strict parsers, empty text for Futures private).
Track-received and disposed checks retain their placement outside strict-parser
catch regions. Parsed messages and original raw data/errors retain identity.
The Futures-private performAuth invocation is a no-argument fire-and-forget
method binding; this body neither awaits it nor owns its asynchronous lifetime.
Ticker/book/OHLC statistics guards, confirmations and private authentication
error/token resets follow the source's lazy access and operation order.

`private_control.rs` ports performAuth and subscribePrivateFeeds using AuthHost.
The host binds the current pending/credential/signer properties, captured signer
method, nanoid, signing promise, SHA256 UTF8 challenge hashing, base64 secret
Buffer decoding and SHA512 HMAC base64 digest. This is orchestration parity;
The hash binding resolves its constructor before invoking the supplied pending-
challenge getter at update-argument evaluation. Cryptographic engine implementations
and permissive Buffer semantics are not
claimed. Remote requests retain native type/exchange/output-format constants,
original request challenge identity and separate current challenge reads after
await. Subscription fields are captured once before the four sends, including
the source's rejected account_balances feed name and undefined signatures.
Prepared native fields are asserted before the host's JSON.stringify/send
boundary. Source catch regions stop the batch and retain their silent handling.

RefreshHost binds live timer tokens, clearTimeout, setTimeout and the two
callback identities. Schedule captures the current Futures fetchThrottledOrders
method during registration, rather than looking it up when fired; the Spot
async closure is likewise captured at registration. Both throttle bodies schedule at 1000 ms, clearing the
current truthy token first. Futures refresh clears its token before disposal,
awaits the original fetch, then reads/spreads current loaded flags with orders
true. The spread/update intrinsic belongs to the host. Spot's captured async
callback calls fetchAndSetOrders without a disposal guard or token reset.
Caught refresh errors remain swallowed exactly as in the source. Neither
callback body claims the concrete browser/Node timer engine or live Store.

Finite actual-source/native floors: 42 cases per dispatch body (168 total),
20 performAuth, 8 private subscription, and 8 cases per refresh body (24 total).
Await witnesses stop after one independently released signing/fetch continuation;
Spot callback witnesses invoke at most one captured callback. Promise assimilation
of thenables is a bound intrinsic, and the oracle does not count Promise-internal
then-property probes as method property operations. Native values, UTF-16 text,
numeric bits, object presence, original message/challenge/error identity and
side-effect order are asserted before any wire-derived subscription assertion.
Complete websocket classes, handler parsers, parsing/coercion engines, remote
signer/crypto engines, timers, concrete Store integration and live sessions
remain separate contracts or rows. Eight compiled defects discriminate nine
rows; the shared 1000->1001 debounce defect is run against separately selected
Futures and Spot witnesses, requiring a named failure and restoration for each.

### Kraken order-book and listener hosts

`orderbooks.rs` ports Futures and Spot snapshot, delta and emission bodies.
BookHost binds JavaScript field/get-key access, parseFloat, source array/string
iteration, current handler/cache properties, Map set/delete/entries, stable
Array.from(entries).sort comparators, market find/contractSize truthy fallback,
ToNumber multiplication, method invocation and Array.forEach. It supplies current
cache instances on every required lookup; required_side captures the selected
Map before its set/delete call. Concrete Map/object-prototype engines
and websocket class fields remain separate contracts. Snapshot filters retain
qty > 0; deltas delete exactly zero and retain negative/NaN quantities. Futures
snapshot reads both raw sides before replacing the cache; Spot replaces first.
Futures records book stats in snapshot and delta, while Spot records only delta.

Rust owns the 100-level slice, per-level native price/amount/total construction,
Futures-only contract scaling call and callback object construction. Each callback
gets a fresh outer book while sharing the original bids/asks arrays. Each level
is also its own Rc<RefCell<OrderBookOrders>>, so references retained after slot
removal remain live and mutable. The witness mutates a removed level in the next
callback and checks it after return. ForEach binds
initial length, current callbacks and holes. Sorting with NaN and SameValueZero
Map keys is a declared intrinsic boundary, not a new general sorting/Map engine.
The native fixture implements ordered finite Maps and finite stable comparator
cases, asserting native cache operations/entries and emitted values before wire.

`book_listeners.rs` ports both listenOrderBook methods and Spot listenOHLCV.
ListenerHost binds current handler-map spread/append and strict callback-reference
filter, current clearFeedStatsIfIdle invocation/map, property key/String coercion,
existing Spot INTERVAL lookup including inherited table identities, connection
state, JSON.stringify/send and setTimeout. The Rust Listener retains original
callbacks and captures. Its recurring poll is the same Rc at every 100 ms
registration. The host owns scheduled lifetime and invocation; disposal does not
cancel the pending poll, matching the source. No disposed guard is introduced.
Futures captures its subscribe payload before connection checks and catches send
errors with the original exception. Spot creates its frame at each send and lets
send errors propagate. Spot disposal reads current handler length after cleanup
and unsubscribes only on exactly zero plus connected. OHLC captures its key and
interval, but reads the original options object's current symbol when sending,
including after disposal-time options mutation.

Finite actual-source/native floors are 17 snapshots, 25 deltas and 21 emissions
per venue, and 18 cases per listener (180 cases total). Polling stops after at most
two captured callbacks and disposal at most twice; array callback enumeration
stops at the originally captured length. Native UTF-16 keys, numeric bits/NaN,
current cache/original options/callback identities, native prepared frame fields,
callback mutation/deletion and operation/checkpoint order precede wire assertions.
Seven compiled production defects require nine independently selected named
failures and restored passes, including both venues under shared snapshot/depth
mutations. Complete websocket classes, general intrinsics, JSON serializer,
concrete cache/handler maps, timer engine and live sessions are not claimed.

### Kraken REST account read hosts

`safe-cex-venue-kraken/src/rest_account.rs` ports eight exchange method bodies:
Futures/Spot markets, Spot tickers/orders, Futures positions/balance/historical
fills, and Spot balance refresh. `ReadHost` binds parsed Axios response data,
original opaque values, dense parsed-data arrays/Object.entries, JS property and
coercion intrinsics, imported asset/order/precision/fill helpers, current Store
slots, preference Maps, clock, HTTP and logging. Body-owned fields, filters,
cache writes, merge/patch construction, numeric calculations, pagination and
fallback branches remain in production Rust. `object`, `spread` and `assign`
retain source own-key order, and opaque references preserve original info and
Store fallback identity. Rust's first future poll corresponds to entry of the
source async body; the test releases the first read after checking that prefix.
These are method body ports, not concrete Axios adapters or complete exchange
classes. They do not claim arbitrary sparse/reentrant JS arrays, exhaustive JS
intrinsics, credentialed REST behavior or live exchange coverage.

`tests/rest_account.rs` executes actual TypeScript methods through
`tests/rest-account-oracle.cjs`, independently checks native numeric bits,
NaN, UTF-16 values, object own-key order, object/error identity, request/update
observations and preference state, before any output serialization. The finite
case floors are 13 Futures markets, 12 Spot markets, 11 Spot tickers, 12 Spot
orders, 13 Futures positions, 14 Futures balance, 5 Spot refresh, and 17 Futures
fill histories. The fill witness stops at the source's 50-page cap, including
an unwindowed first request, dedupe/window filtering, cursor, disposal, warning
and limit paths. `scripts/rest-account-controls.py` introduces seven production
defects with eight named test selections and restores the original source.

### Kraken trading REST and API hosts

`safe-cex-venue-kraken/src/trading.rs` ports Futures/Spot placement and amendment,
Futures cancellation, leverage and margin changes. Production owns request fields,
acknowledgement checks, protective-order provenance refusal before cancellation,
replacement ordering, trailing-stop validation, clamping, cache changes and error
refinement. `TradingHost` binds imported adjust/ID helpers, original options,
current Store values, XHR versus unlimited XHR and parent dynamic methods.
`ErrorHost` preserves original errors, response and cause handles and source
String-method semantics; contextual expression labels preserve intrinsic errors.

`src/api.rs` ports both API factories and their request/response interceptors.
Production builds constructor/retry options, chooses base/network, installs the
Futures transport conditionally, registers native `Api` request/success/rejected
methods, applies signing guards, retains original unsigned config identity,
mutates signed bodies/query URLs in source order and constructs headers/errors.
`ApiHost` binds Axios creation/registration, imported retry helpers, qs encodings
(`Sorted1738` means repeat arrays, the existing alphabetical comparator and
RFC1738; `Repeat` means the source Spot options), JSON/URI, Buffer and crypto.
The native factory returns an `Api` wrapper retaining the original client and
registered method bodies. One `Rc<FuturesModule>` represents a TypeScript module
instance and MUST be shared by every Futures client, including history clients
and different exchanges in that instance. Its counter uses double increment and
modulo semantics. Each Spot `Api` owns its separate double counter. The first
poll of a native trading future corresponds to source async entry.

`tests/trading.rs` compares native values before any serialization, including
numeric bits/NaN, request fields and key order, call/await order, current cache
state, original input/error/response/cause identity, direct uppercase refusal,
protective provenance and repeated-client nonce materials. Its finite floors are
37 Futures placement, 29 Spot placement, 21 Futures amendment, 24 Spot amendment,
12 cancellation, 18 leverage, 20 margin, 33 Futures API and 34 Spot API cases:
228 total. The oracle selects unchanged actual TypeScript bodies and actual
trigger/get-key/adjust helpers. Imported qs/JSON/URI/Buffer/crypto calls use
explicit recorded sentinel bindings, so this proves body-owned arguments and
sequence, not third-party encoding/crypto correctness or valid live signatures.
`scripts/trading-controls.py` compiles eight defects in disjoint families together,
requires nine named decoded-native assertion failures and restores nine passes.
No sparse/reentrant-array, exhaustive intrinsic, concrete Axios, full exchange
class, credentialed trading or live signing/transport claim is made.

## Kraken private account and income wrapper hosts

`safe-cex-venue-kraken/src/private_accounts.rs` ports the four Futures private
account handlers and Spot executions/parser bodies. `AccountHost` extends the
opaque original-value `ReadHost` boundary: JavaScript property/get-key lookup,
coercion, own-key order, ordinary-object prototypes, dense parsed array iteration,
addition, direct `toString`, Date construction and imported position-ID creation
remain intrinsics. Store operations, emitter, current parser and throttle calls
are bound dependencies. The bodies construct the source fields themselves,
retain original `info` and opaque IDs/symbols, query current Store rows for each
item, and propagate callback errors. Optional preference Maps are distinct from
REST's required Maps. Websocket cash/flex/flat balance handling is its own source
body, including unclamped balance-value free margin and no synthetic USD record.

The two exchange income wrappers use `IncomeHost`: the existing imported pager,
account-log page body, current range method, clock, ISO Date construction and
warning/error methods are bound dependencies. Rust builds pager options and its
page callback, checks truncation, filters records, scopes/limits after await,
and preserves the source catch boundary. This adds no income pager or live
ledger/network claim. The existing shared `safe-cex-exchange/src/income_paging.rs`
port is CHOSEN divergent at its typed owned record/options, UTF-8 and snapshot
boundaries. These wrapper witnesses supply a controlled pager dependency, not
an end-to-end adapter to that native pager. Concrete websocket/exchange
classes and full shared-pager assembly remain separate runtime work.

`tests/private_accounts.rs` runs the actual TypeScript arrow bodies selected by
AST, then checks native numeric bits/NaN, UTF-16, own-key order, original `info`
and exception identity, cache effects and callback order. Its finite fixture
floor covers ordinary parsed values, explicit undefined, malformed rows,
reentrant Store/option changes and synchronous/asynchronous failure. It does
not enumerate custom accessors, exotic prototypes, sparse arrays or scheduling
beyond the declared bindings. `scripts/private-account-controls.py` compiles
one disjoint defect per family together, requires each named native failure,
then restores exact source bytes and requires all eight named passes.

## Kraken externally assembled class factories, constructors and OHLCV polling

`safe-cex-venue-kraken/src/classes.rs` provides six executable class factories
through `ClassHost`. They construct a superclass receiver, define 113 own
fields across the six classes in source order (including 85 fresh bound arrows),
and preserve the Futures public prototype `mapTimeframe` separately. Non-declare
uninitialized fields are explicit undefined; declare websocket fields are
omitted. Map/object allocations are fresh, Spot grafana arrays are imported
module identities, and detached arrows retain the original receiver. DefineField,
function allocation/naming, superclass/prototype behavior and value ownership
are explicit runtime bindings; this is not a concrete JavaScript heap or GC.

The six class rows are **CHOSEN divergent**: source class declarations fix their
own member bodies and can be constructed with ordinary parent or
name/options/store arguments. These native factories instead require complete
externally supplied callable member bindings over the separately mapped native
ports. Missing bindings produce a named construction failure. This explicit
assembly contract reuses the maintained heterogeneous native member interfaces
without pretending that a fixed production runtime adapter has been shipped.
All own callable slots and the prototype method require concrete callbacks;
there are no deferred missing links or successful partial instances. Existing
ported/divergent member boundaries remain those of their own rows. The target
strings are binding diagnostics, not evidence that every body is linked by this
factory. The native factory is executable when the caller supplies those bodies;
it is not an unqualified port of fixed source class composition.

`construction.rs` ports the two constructor bodies. It calls the superclass,
then the derived initialization phase, then the secondary-key client helper
with the appropriate API factory and a callback requesting maxRPS 5. It assigns
xhr after that helper returns, then unlimitedXHR, and Futures historyXhr last.
Original options/store/name/receiver handles and callback ordering are retained.
Superclass initialization, existing imported API factories (including module
nonce ownership), the secondary-key helper and rate limiter are dependencies.
The controlled constructor witness proves forwarding and assignment order;
it does not prove native API/signing integration or a fresh nonce-engine gate.

`ohlcv_polling.rs` ports Futures exchange listenOHLCV. It captures the original
options/callback and UTF-16 key, clears an existing timer, marks the key active,
and starts the same poll callback immediately through the host task engine.
Fetch options use current original option fields, limit 2 and current clock.
It selects the last candle and compares timestamp/close/volume with strict
source equality. Callback runs before caching the original candle. Callback or
fetch failure logs the original cause. Cleanup during an in-flight fetch still
permits emission/cache insertion after await; only later scheduling is guarded.
Immediate and suspended task rejections remain out of band; listener cleanup is returned.
Timers reuse the same function with delay 5000. Old cleanup can deactivate and
cancel a replacement sharing its key, matching source rather than adding a
new generation policy. Timer/Map/coercion/Promise ownership is host-bound.

`tests/composition.rs` evaluates actual class declarations/arrow bodies and
checks native original identities, field order, callback freshness/receiver
capture, shared imports/fresh containers, constructor effects/errors and poll
state/continuations before wire claims. Each class has paired ordinary source
construction versus native rejection of missing bindings. Complete test bindings
exercise real native message/account/disposal/guarded-tick/timeframe chains;
other callable bodies in these class-factory fixtures are controlled external
callbacks, not a claim of fixed production linkage. The finite fixture floor
is 27 class cases, 16 constructor cases and 16 poller cases (59 total), with at
most 20 native continuation polls per step and one source microtask flush per
phase. Fixture-declared shared candle references are restored before native
execution; JSON transport does not define their identity. Exotic descriptors,
prototypes, sparse arrays, GC, networking and general scheduling are outside
this floor. `scripts/composition-controls.py` requires nine named native failures
and restored passes for three disjoint class-order/constructor-rate/poll-delay
family defects.

## Orderly websocket control bodies

`safe-cex-venue-orderly/src/ws_controls.rs` ports ten arrow bodies: public/private
connect, open, subscribe and ping, plus public onMessage and handlePingEvent.
`WsHost` retains opaque original receiver, options, data, parsed JSON, topic
values, functions and errors. Runtime member calls read current callable slots,
pass the original receiver, and propagate the original error. Class construction,
JSON/string/number coercion, property access, own enumeration, one-level lodash
flatten, string includes, live callback iteration, clocks, signing and imported
configuration remain explicit host dependencies; a host must implement their
source contracts. This is a body port, not fixed native class assembly.

Connect selects the existing public/private livenet/testnet constant, interpolates
applicationId, assigns the created socket before installing open/message/close
listeners with original callbacks, and installs no error listener. Public open
pings, sends four built-in topics, invokes current subscribe, then conditionally
emits reconnect. Private open reads virtual time, signs the timestamp string with
the original secret, sends auth, then invokes the current parent snapshot method
without awaiting it. Public subscribe has no disposal guard and flattens one
level in source own-key order. Private subscribe sends three topics then pings.
Pings guard disposal, assign monotonic time before send, and retain public versus
logged private send. Public handlePingEvent is unguarded. Public dispatch captures
event data before checking disposal, tracks receipt, snapshots handler entries,
uses the source substring heuristic, parses only the first matched topic, and
iterates that original handler live. It invokes callbacks without a receiver and
with the same original parsed handle, checking truthiness on each iteration.

The paired actual-arrow/native witness has 62 finite cases across ten bodies:
11 connect, 12 open, 11 subscribe, 16 ping/pong and 12 message. It asserts native
UTF-16, number bits/NaN, field shape/order, original callback/socket/parsed/error
identity, first-match behavior, one-level flatten and live handler append versus
entry-snapshot replacement. Controlled stringify returns its original argument
so native payload assertions precede any encoding; it is not a codec witness.
Parser/signing/clocks/base effects are also controlled imports. Exotic accessors,
proxies, non-string includes, sparse arrays, malformed/general JSON behavior,
crypto/nonce integration, promises/network/reconnect engine and classes remain
outside this finite floor. Native body behavior depends on faithful host
intrinsics; no wire or live exchange claim follows from these cases.

## Orderly stream and report bodies

`safe-cex-venue-orderly/src/stream_reports.rs` ports eight bodies: public ticker,
BBO, mark-price and funding-rate handlers; private message dispatch, position,
execution and algo execution reports. `ReportHost: WsHost` retains original row,
mapper result, ticker, store, emitter, callable and error handles. Property
access, strict equality/truthiness, normalization, get-key/parseFloat, array
forEach and live iteration, JSON and runtime effects are explicit dependencies.
Array forEach captures initial length and reads current indexed values; for-of
iterations remain live. Mappers remain separately pending rows. These body ports
do not claim a fixed native class/mapping/network adapter or fresh import gates.

Public handlers filter PERP_ rows, normalize the current row symbol, look up
tickers through parent.store, and update through this.store with source patch
fields and source values. They have no disposal guard. Funding uses get-key and
parseFloat. Private dispatch captures data before its disposal guard, tracks
receipt, emits diagnostic data, then parses. Ping/pong/auth precede report topics,
which receive the original parsed data; auth invokes the existing subscription
body. Positions accept only array-valued positions and skip falsy mapper results.
Execution maps first, then reads status/price/amount, accepts both partial-fill
spellings, adds/status-updates before fills, and preserves original mapper and
fill values. Algo reports accept scalar or array input, map active orders, emit
FILLED before closing root/child orders, and preserve truthy ID fallback and
source side/normalization/child iteration semantics. There is no added catch.

The actual-source/native finite floor is 86 cases: four public families of eight,
16 private dispatch, eight position, 15 execution and 15 algo cases. Source
oracle selects unchanged arrow bodies and actual normalization/get-key/enum
declarations; native fixtures assert original handles, UTF-16, numeric bits/NaN,
patch/fill key order and source effects/state before encoding. Trace snapshots
copy native values after identity checks, independently of wire encoding, so
later mapper mutation does not rewrite earlier observations. Imported mappers,
base effects and JSON are controlled bindings. Tests include mapper-mutated
status, append during forEach versus live position iteration, alternate key
spellings, partial fills, root/child status and first dispatch precedence.
Exotic accessors/proxies, sparse arrays/custom forEach, malformed/general JSON,
crypto/nonce, general promise scheduling, live networking and fixed class/mapping
integration remain outside this finite floor.

## Orderly account and order REST read bodies

`safe-cex-venue-orderly/src/account_order_reads.rs` ports eight bodies: position
mode, account, validation, leverage, margin types, limit orders, algo orders and
combined orders. `AccountReadHost: WsHost` retains original receiver, response,
row, mapper results, return handles and error metadata through explicit runtime
call/await boundaries. XHR get, Promise/thenable engine, property access, arrays,
spread/reduce/live iteration, normalization, get-key and mappers are dependencies.
The class and mapLimitOrder/mapAlgoOrder remain separately pending. There is no
fixed production HTTP/Promise/class/mapping assembly claim.

Position-mode cache skips request on truthy current mode; a miss awaits ACCOUNT,
sets one-way and returns the current field. Account forwards original account_id
to userId and uses null for falsy affiliateId. Validation logs API message or
root message, then rereads API msg/root message/fallback for the returned error.
Logger failure propagates. Leverage stores original truthy max_leverage or 20,
then sets one-way. Margin types accept only an array, live-iterate string symbols
with exact CROSS/ISOLATED values, and invoke current mapMarginType and Map setter.
Limit and algo reads send INCOMPLETE params, reduce source rows, preserve current
mapper calls, and log/return empty arrays on request or reduction failure. Limit
orders skip only null mapper results; false, undefined and zero remain members.
Algo orders spread children. Combined orders await limit completion before
calling algo, then spread limit values followed by algo values; no Promise.all.

The actual-source/native witness has 71 finite cases across eight named tests
(8 mode, 8 account, 8 validation, 9 leverage, 8 margin, 12 limit, 9 algo, 9 combined).
It pins synchronous prefixes, original response/row/mapper/error identities,
UTF-16, numeric bits/NaN and ordered calls/results/state before encoding.
Controlled awaits yield once and apply declared state changes before resolve;
validation log mutations demonstrate rereading error fields. Reduce captures
initial length; margin for-of observes append. Combined reads include real
native/source limit/algo body chains over controlled requests/mappers. Source
normalization/get-key/enum/endpoint declarations are evaluated unchanged. Native
trace snapshots copy values directly after identity checks. Unknown accessors,
proxies, sparse/custom arrays, malformed response structure, general thenables/
scheduling, crypto/nonce and live network/mapper integration remain outside the
finite floor. These host dependencies require their source contracts; this is no
fresh gate of already-resolved import rows.

## Orderly trading action bodies

`safe-cex-venue-orderly/src/trading_actions.rs` ports eight bodies: updateAlgoOrder,
cancelOrders, cancelAlgoOrder, cancelAllOrders, cancelSymbolOrders, setLeverage,
setAllLeverage and changeMarginType. `ActionHost: AccountReadHost` retains opaque
original order, sibling, symbol, margin, position, request and error values.
Source property/in/strict equality, Number.isFinite, Number/parseInt/Math, array
find/filter/map, forEachSeries, Error creation, symbols and HTTP/await effects
are runtime/import dependencies. There is no fixed class, HTTP/Promise engine
or live trading adapter claim.

Algo editing builds numeric parent ID and optional child price before its try,
then catches put/return failures as logged empty IDs. Serial cancellation
delegates algo orders and sends normal orders with parsed ID/query/data config;
errors propagate. Algo cancellation captures the first sibling before await,
then uses its current price/type/margin fields to recreate it. Its catch handles
'already canceled' as status update and logs other causes. Bulk/symbol cancellation
catch each endpoint separately and read current XHR for the second call; symbol
normalization happens once before the catches.

Single leverage validates market/finite input, reads current margin and clamps/
rounds before its try; strict success true is required, only cross updates the
account cache, and original matching positions receive patches after await.
Single leverage and margin errors log then rethrow the original cause; all-market
leverage catches request failures and skips equal/disposed state. Margin change
validates symbol/market before try, then updates its cache only after success.
Generated Error metadata and original caught error identity remain distinct.

The finite witness has 76 cases: 8 edit, 8 serial cancel, 9 algo cancel, 7 bulk,
7 symbol cancel, 14 single leverage, 13 all leverage and 10 margin. It evaluates
actual arrows/enum/endpoint/symbol/get-key declarations and installed forEachSeries,
checks native UTF-16/number bits/NaN, payload/result/state/effect order and original
position/error identities before encoding. Await mutations exercise current XHR,
sibling price and position reads, plus append during serial cancellation.
Controlled requests yield once; array/Promise/
Error host contracts are bindings rather than a general runtime gate. Exotic
accessors/proxies, sparse/custom arrays, promise-valued series entries, arbitrary
coercions/iterators and general scheduling/network/crypto integration remain
outside this floor. No existing dependency row is regated by this unit.

## Orderly data and mapper bodies

`safe-cex-venue-orderly/src/data_mappers.rs` ports nine complete bodies: markets,
tickers, positions, position/limit/algo mappers, candles and two subscription
delegates. `DataHost: AccountReadHost` retains opaque original response, row,
store, callback, cleanup and error values. Property access, Number/Math/operators,
array operations, deep cloning, object spread, uniqueness, symbol/get-key helpers,
order tables, safe subtraction, clock and HTTP/await effects are source-contract
bindings. No shared SDK types or traits change; fixed class/runtime/HTTP assembly
is separate work. Previously ported imports are dependencies, not fresh gates.

Markets filter perpetual symbols, split asset names, default finite leverage
limits and clone raw info. Tickers preserve raw close and row info, round change
percentage in source order and return current cached tickers after logging.
Positions await mode after response destructuring, reduce real positions, append
long/short placeholders, spread current balance before subtraction, and retain
first IDs. Its catch logs then returns current cached positions. Position mapping
keeps finite guards, sign/PnL rounding, raw ticker/account fallbacks, leverage/imr
priority, current margin/ID calls and clone-before-clock ordering. Limit mapping
retains nullish executed/reduce fields and original info; algo mapping retains
truthy aliases, child filtering, parent precedence and cloned info with inherited
margin mode. Candle mapping divides timestamps and reverses the mapped array;
subscription delegates forward original arguments/callbacks and cleanup handles.

The finite actual-source/native witness has 66 cases in nine named assertions:
7 markets, 8 tickers, 11 positions, 11 position mapper, 7 limit mapper, 10 algo
mapper, 6 candles and 3 per delegate. Actual arrows and source enum/endpoint,
normalization, get-key and safe-math declarations are selected from six hashed
SDK files. Native UTF-16, numeric bits/NaN, ordered results/effects/state and
original raw-info/cleanup/error/cache identities are asserted before encoding.
Returned cloned info roots and nested fields are distinct from originals. An
integrated position body case and post-await market/balance/leverage changes
exercise current dependencies. Ordinary array snapshots and explicit await
yields are fixture bindings; exotic accessors/proxies, sparse/custom arrays,
arbitrary coercions/iterators, malformed responses, general thenables/scheduling
and network/crypto integration remain outside this finite floor.

## WEEX externally assembled class factories

`safe-cex-venue-weex/src/classes.rs` assembles the three remaining WEEX class
rows through required callable `ClassHost` bindings. All 45 exchange arrows,
20 public arrows, public prototype `dispose`, and 14 private arrows must resolve
before superclass construction. Missing bindings and mismatched target labels
produce named errors. Exchange and public factories reuse the maintained native
constructor bodies through construction-only runtime projections and an injected
arrow binder; private assembly supplies the source implicit constructor and six
own data fields. Each bound arrow retains the same receiver and host, forwards
arguments, and returns the supplied native value or owned future. Prototype
`dispose` receives its caller's receiver; the prototype host primitive installs
once per host class scope and preserves the callable on subsequent instances.
Imported base classes, collections,
constants, child construction and opaque heap ownership are host capabilities.

The class rows are **CHOSEN divergent** under the same external assembly boundary
as Kraken: source declarations fix member bodies and accept ordinary constructor
arguments; these native factories require a complete externally supplied callable
adapter table across the existing heterogeneous body interfaces. Target labels
check declared provenance, not supplied callback semantics. Fixed production
SDK/body adapters, JavaScript heap/GC and arbitrary scheduler behavior are not
claimed. The bounded class witnesses exercise every arrow's receiver/argument
binding with observer adapters, plus actual native `is_algo_order` and public /
private `ping` body adapters. Their native state/value assertions precede any
wire assertions; the source oracle instantiates actual unmodified class bodies.
Public prototype `dispose` is tested for receiver dispatch, not full body wiring.
Existing method-row proof boundaries remain independent of this class census.

## Orderly runtime and order execution bodies

`safe-cex-venue-orderly/src/runtime_actions.rs` ports eight bodies: transient
validation classification, dispose, start, tick, updateOrder, placeOrder,
placeAlgoOrder and placePositionalAlgoOrder. Venue-local `RuntimeHost: DataHost`
retains opaque receiver/store/rows/options/response/error handles. Property and
array operations, own keys, strict equality/Object.is, Number/Math/operators,
precision adjustment, imported tables/helpers,
mapSeries/times/sumBy, HTTP/await, generated Errors, logging options and loop
scheduling remain source-contract bindings. Class/constructor assembly and full
HTTP/Promise/timer/network implementations are separately pending. No shared
SDK trait or type changes; imported resolved rows are dependencies.

Startup captures public-only mode, classifies validation failures, deactivates
only definitive rejections, and updates markets/tickers/private data in source
order with the original disposal checks. It awaits margin-mode synchronization,
registers a detached leverage catch, connects sockets, then polls and loads
orders. `LeverageCatch` supplies the native error callback body. Disposal sets
the flag before disposing public then private sockets. Tick polls positions,
rounds summed PnL, patches current balances, loads orders every fourth count,
and schedules only when still active. `TickCallback` resolves the current tick
method and applies source count+1 semantics when invoked. Imported async loop
registration is detached: scheduling rejection is observed out of band, while
getTickInterval failure still rejects tick.

Amendment delegates algo orders first, otherwise validates one changed field,
reconciled stable fields, ordinary GTC LIMIT shape, precision, own REST provenance,
canonical immutable ID, zero fills, visibility and exact precision alignment.
Short-circuit guards retain later-field read suppression and current precision
reads. It sends only the changed price/quantity and requires success true plus
EDIT_SENT; logged failures preserve the original cause. Placement resolves caller
ID before delegation, derives margin/tag/position side, splits adjusted lots,
checks single-ID constraints before requests, maps POSTs serially and appends
optional positional protection. Algo placement rejects unsupported trailing
stops, selects/cancels the first matching sibling, then uses current options to
place numeric stop/take fields. Positional protection keeps nullish margin-mode
fallback, number-only children, string trigger prices, raw returned IDs, and
logged empty-list failure behavior with the source rejection-log options.

The finite actual-source/native witness has 110 cases in eight named tests:
10 transient, 4 disposal, 16 startup, 16 polling, 25 amendment, 18 placement,
10 algo and 11 positional. Source arrows/import declarations in eight SDK files
and installed p-iteration are hashed (nine files). Native UTF-16, numeric bits/
NaN, original input/mapper/delegated-return/error/promise handles, synchronous
prefixes, ordered effects and current state are asserted before encoding.
Focused witnesses cover a throwing later precision getter skipped by an earlier
NaN guard, detached scheduling rejection, leverage catch fallback/logger failure,
post-await XHR/options/tick replacement, disposal checkpoints, serial lots and
strict versus Object.is zero-fill guards. Other exotic accessors/proxies, sparse/
custom arrays, arbitrary coercions/iterators, malformed response structures,
general thenables/event-loop ordering and crypto/live exchange integration
remain outside the finite floor. Controlled runtime/import implementations are
dependencies, not a general interpreter or fresh gates for their older rows.

## Orderly exchange class factories

`safe-cex-venue-orderly/src/classes.rs` ports the exchange constructor and seven
class initializer/composition factories: Orderly plus KodiakFI, MODE, SalsaDEX,
TealstreetDEX, What and Woofi. This is an adapted source class-factory surface
over mandatory source-contract bindings, not a self-contained concrete runtime
class. All 39 member ABIs (38 own arrows and one prototype method), superclass
construction, API/rate-limit/socket construction, own-field definition and
imported rule/endpoint objects must be provided by the host. Member provisioning
is infallible and has no observable effects; absent/incompatible bindings are
outside this contract, not an extra constructor rejection path. Prototype
registration installs once and preserves the original member on later instance
constructions. Field definition follows source own-property descriptor semantics.

Options are shallow-spread into a fresh object; truthy key/secret values gain
the ed25519 prefix only when absent, before superclass construction. The original
name/store and nested option values are retained. Source order installs 48 own
fields including 38 fresh lexical arrows, an empty Map, per-instance rule and
shared endpoint arrays; getPositionMode stays on the prototype. API creation
receives the same normalized options and receiver, then rate limiting at 10 RPS
and public/private socket construction follow in order. Derived classes forward
constructor arguments and newTarget through the base exchange, then override
only the source exchange-name field after inherited construction. Imported
constructors see the initial orderly name before each derived override.

The actual-source/native witness has 60 finite cases: 11 constructor and seven
for each of seven class factories. It evaluates unchanged actual class
declarations from nine hashed SDK files, including the actual pattern-rule
helper. Independent floors are 49 definitions, 48 own fields, 38 arrows and one
prototype method; each instance includes six controlled superclass fields.
Native values/descriptors/key order, numeric bits/NaN, UTF-16, original error
identity/metadata, shallow option sharing, fresh arrows/Map/rule, shared imports,
constructor effects and caller options are asserted before encoding. A detached
arrow calls the existing native candle delegate with original arguments and
returns original cleanup; the cached native prototype mode body uses a foreign
receiver and returns its original mode. The other 37 member ABIs are mandatory
external dependencies, not shipped invocation-adapter coverage or fresh gates
of their already-resolved rows. Existing inherited member divergences stay
classified in their own rows; this factory slice does not repair or conceal them.

No fixed complete member/HTTP/websocket/signer/runtime/collector assembly claim.
Only these two member chains are invoked. Other superclass/import behavior,
exotic property accessors/proxies, arbitrary option types, host allocation/GC,
networking and source class inheritance across foreign runtimes remain outside
the finite witness. This venue-local trait changes no shared SDK type/trait.

## Orderly API and public feed bodies

`api.rs`, `public_feeds.rs` and `socket_classes.rs` finish seven source rows:
request signing, API creation/interception, candle/book listeners, book delta
processing and public/private websocket initializer factories. The adapted
surface keeps opaque original values, live property/member reads, lexical
callbacks, error handles, mutable options and async snapshot state. Hosts must
supply source-contract intrinsics/imports: ordered spread/JSON/qs/join, Axios
headers/client/retry/transport, clock/signing, array iteration, normalization,
book insertion/totals, superclass/descriptor/member bindings and timers. Async
spawn polls the source prefix eagerly and observes detached rejection out of
band. These are mandatory bindings rather than a concrete assembled runtime.

Signing makes a shallow config copy, uppercases/defaults method, serializes
truthy params once into the signature URL, signs timestamp/method/URL/body and
constructs overriding headers. API setup preserves nullish recvWindow, sorted
query serialization, retry-before-transport order and the original mutable
options in its interceptor. Public URLs return the original config; POST order
and algo bodies mutate the original data with current network broker IDs.

Subscriptions register exact parsers and share topics/handler lists. Candle
cleanup leaves its pending retry alive, as in TypeScript. Book cleanup clears
its latest timer and shared book/buffer but does not cancel in-flight REST.
Snapshots filter updates strictly newer than snapshot timestamp, apply original
buffered frames, calculate totals and return the same mutable book handle to
every callback. Request failure clears the buffer before logging/retrying; a
synchronous failure runs that catch before the first parser stores its frame.
Detached logger failure is observable through the host's rejection monitor.

Websocket factories forward all superclass arguments, define own properties in
source order and install fresh lexical arrows. Public initialization adds fresh
topics and six handler arrays whose callbacks dispatch current methods on the
original instance; private initialization adds eight arrows. All 21 member ABIs
are mandatory external bindings. The finite factory witness invokes the six
public wrapper dispatches; other member invocations remain dependencies. It
asserts 15 public and eight private initialized own fields, plus one controlled
superclass field, their descriptor flags/order/values and per-instance freshness.

The actual-source/native target has seven tests and 41 named finite cases.
Native ordered object/array values, UTF-16, number bits/NaN, mutation state,
callback book sharing, original errors and ordered effects are asserted before
encoding. It evaluates unchanged source bodies/classes and imported source book
helpers/reverseSymbol. Query inputs stop at ordinary ASCII scalar records;
AxiosHeaders and HTTP/retry/transport/signing are controlled import bindings.
Source utility helpers are dependencies, not newly resolved parity rows.
Ordinary dense arrays and finite numeric/raw string levels are the book floor.
One throwing URL coercion witnesses conversion before query serialization.
Other exotic accessors/proxies, custom iterators/coercions, sparse arrays, arbitrary
thenables/event-loop schedules, dependency implementations beyond that floor,
full HTTP/websocket/class/signer assembly and live exchange are not claimed.
No shared SDK type or trait is changed.

## Bitrue account, order and history read bodies

`safe-cex-venue-bitrue/src/collection_reads.rs` ports nine complete source read
bodies: open positions, selected positions with metadata, balance/positions,
serial order reads, background order reconciliation, per-symbol orders, private
trade rows, internal historical trades and public historical fills. The adapted
surface keeps original opaque values/errors and source-contract runtime/import
bindings. Underlying account/position parsers and helpers, HTTP, array/Set/number/
string/date/table intrinsics, mapSeries, clock/sleep and rejection monitoring
remain dependencies. The pending parsePositions row is not claimed here.

Position wrappers preserve original fetched/parsed handles and current member
lookup after await. Selected positions retain original matched position rows,
fetch metadata for missing symbols (including source duplicate behavior), spread
metadata after the constructed ID and install the five fixed zero fields.
No Store-wide borrow guard is owned by these bodies. Per-symbol order failure
logs the source error and returns original Store orders for that exact symbol.
Order mapping retains raw amount/filled/price/time/info and the source conditional
order/status rules. Unspecified order symbols warn and start detached refresh;
rejection monitoring observes the source unhandled rejection out of band.

Background reconciliation snapshots market symbols into insertion-order Set
semantics, deduplicates with SameValueZero, then reads current priorities and
fetch-generation ID each iteration. It uses the authenticated helper client
through its underlying per-symbol reader, checks disposal after fetch and sleep,
and passes original orders to current Store update methods. A superseding fetch
logs cancellation at the next source checkpoint. Sleep remains 100ms.

Trade requests preserve original options, clamp source truthy limits and omit
only undefined parameters. History keeps nullish timestamp precedence, the
strict millisecond threshold, source numeric/ISO date conversion, quantity/price/
time guards, symbol/order/time filtering, stable descending sort and limits.
Per-fill IDs, booleans and finite absolute commissions survive the final fill
mapping; public execution timestamps use floor(milliseconds/1000). Catch paths
log API message before root message and preserve logger failures.

The paired actual-source/native target has nine named tests and 53 finite cases:
3 open, 6 positions, 3 balance, 5 orders, 7 refresh, 7 per-symbol orders, 8 raw
trade reads, 8 internal history and 6 public fills. Six SDK files are hashed;
unchanged arrow bodies, enums, tables, position-ID and omission helpers run in
the oracle. Native ordered values/arrays/UTF-16/numeric bits/NaN, errors, original
row/info/position/balance handles, effects, retained results, Store updates and
post-await state are asserted before encoding. Existing native read helpers
serve bounded import bindings; their older divergence classifications remain.
Underlying method calls are controlled dependencies rather than full exchange
composition. Ordinary dense arrays, ordinary own properties, primitive values,
ASCII table/symbol/date fixtures and finite async schedules define the floor.
One supported ISO date and invalid-date cases witness Date.parse binding;
arbitrary dates, sparse/custom arrays, proxies/accessors, prototype table hits,
arbitrary iterators/coercions and event-loop schedules remain outside it.
No shared SDK type/trait, fixed native exchange/Store/HTTP assembly, transport
or live exchange claim is made. Production balance-retention regression remains
separate and is run scoped when gating this Store-writing read family.

## Bitrue REST trading and market bodies

`safe-cex-venue-bitrue/src/trading_rest.rs` ports nine complete source bodies:
fetchMarkets, fetchOHLCV, formatCreateOrder, placeOrder, placeOrders,
placeOrderBatch, updateOrder, setLeverage and cancelOrders. TradingHost extends
the venue-local ReadHost with source-contract imports/intrinsics: ordered
copy/spread/rest, find/flatMap/chunk/times/push, property membership, arithmetic,
precision adjustment, UUID, inverse type lookup, Date.now, async callbacks and
Promise.all. HTTP, worker calls, client-ID guards and underlying methods are
mandatory dependencies, not a concrete exchange or scheduler assembly.

Markets retain raw symbol/multiplier/info, strict numeric status and source
precision/limits/fees. Candles preserve strict cache age and ready-state guards,
100 retries at 100ms, post-await current socket mapper/cache writes and original
cached/mapped/worker result handles. Formatting keeps found-position leverage/
margin, source logging/errors, conditional UUIDs, adjusted lots/remainder,
per-lot string volumes, client-ID checks, market quote sizing and attached TP/SL
fields. Attached zero values are included but alone do not trigger worker refresh.
Source lot-sizing quirks are retained rather than corrected.

Single/multiple placements preserve original caller options, guard-before-format
order, immediate flat-map flattening, current post-await options, source symbol
Set order/deduplication and original batch results. Batch placement starts each
lot of five concurrently through unlimitedXHR, appends raw IDs in completion
order and starts the next lot after Promise.all. Failure logs then rereads its
reason before constructing the replacement Error. Cancellation starts lots of
50 concurrently through authenticated xhr, selects truthy native client IDs,
uses serial lots, then sends original order IDs to removeOrders in the worker.
Promise rejection does not cancel submitted siblings. Update retains shallow
original order fields, honors property presence (zero/undefined included),
awaits cancellation and calls the current place member. Leverage invalidates the
metadata cache only on success and retains cap updates, silent logging and
replacement error behavior on failure.

The actual-source/native witness has nine tests and 60 independently fixed
finite cases: markets3, candles7, formatting14, placement6, multiple-placement5,
batch7, update5, leverage6 and cancellation7. Six SDK files are hashed; unchanged
arrows/enums/tables and math/omission/inverse helpers run in the oracle. Controlled
deferred responses exercise reverse completion,5/50 boundaries, sibling
continuation after failure, field/option/method replacement after awaits,
formatter-array reentry, cache expiry, exact timeout floor and empty error
metadata (new Error(undefined) has an empty message). Native ordered values,
UTF-16, numeric bits/NaN, original errors/results/market info/caller handles,
effects and final state are asserted before encoding.

The floor stops at ordinary dense arrays and own properties, ordinary primitive
values/ASCII symbols, bounded finite times inputs, deterministic UUID/clock/math
bindings and controlled request completion schedules. Intrinsic implementations
are dependencies; prototype/symbol/accessor behavior, arbitrary coercions and
iterators, unbounded times, GC and general event-loop scheduling are outside the
finite witness. Already-ported helpers retain their prior classifications.
Underlying method bodies are controlled in wrapper tests; full composition of
all nine adapters is not claimed. No shared SDK type/trait or fixed complete
exchange/Store/HTTP/worker/signer/live-network assembly claim is made.

## Bitrue private authentication and stream bodies

`safe-cex-venue-bitrue/src/private_auth.rs` ports createAPI and the private
websocket factory, incrementalFills field initializer, connection, message
routing, order updates, listen-key acquisition and refresh. PrivateHost extends
the venue-local ReadHost with native intrinsics and mandatory original imports,
clock/JSON/query/HMAC/header/transport/retry/superclass/class-environment/timer
bindings. It changes no shared SDK trait. Underlying method calls are current
source dependencies; the factory provisions all15 exact existing body ABIs as
fresh lexical own arrows after the superclass and field initialization. The
factory supplies the exact250/500/1000ms RETRY_DELAYS initializer and existing
dispose_private ABI to its caller-owned class environment, which retains the
persistent static retry array and prototype super-disposal infrastructure. Those bindings are part of this adapted factory,
not a claim that Rust implements a JavaScript object/promise runtime or invokes
all15 members in one assembled exchange. Detached native ping is witnessed.
The field initializer returns the original shared utility callable; it does not
claim a new port of that utility's algorithm.

The API preserves original options/client/exchange identity, captured nullish
recvWindow, unsorted query serialization, tunnel-before-retry bigInt selector,
original signature-retry binding and interceptor receiver. Signing retains
shallow data/params handles, mutable options/secret reads, public/remote bypass,
first .com split, optional method receiver, exact body/path/time/HMAC/header
order and captured recvWindow. FINDING ts-bug DOUBLE PARSE remains preserved
through the mandatory original safeCexAxiosCreate/installTransportAdapter
bindings; the existing reproduction f6de9ca8c4 is not remeasured or corrected.
No replacement of shared RestResponse parser provenance is claimed.

Connection rechecks disposal after fetch, assigns the original socket and
resolves current listener/handler members. Message routing keeps diagnostics
before parsing, original exception console arguments, ping-before-stats routing,
strict acknowledgement short circuit and malformed-parser-null behavior.
Order updates keep the raw-versus-scaled quantities and incremental raw z
across the conversion boundary; replay emits no fill. Positive fill precedes
Store write, ID coercion precedes reduce/timestamp reads, and original info is
retained. Listen-key acquisition reads live retry-array length/index at source
positions, including mutation during await. Invalid/network/scheduling errors
retry; logging/delay errors escape. Refresh has only the initial disposal guard,
so disposal during await does not suppress successful scheduling; failure emits
the source generic visible error and logger errors escape.

The actual-source/native witness has eight tests and independently fixed finite
floors API10, class4, initializer2, connect6, message13, order10, acquire10 and
refresh7 (62 cases). The floor ends respectively at retry-error,
incremental-error, factory-error, retry-error, diagnostic-error, store-error,
mutable-delays and logger-throws. Eight SDK files are hashed; unchanged source
arrows/class, enums/tables, symbol and incremental helpers and parser run in the
oracle. Signing compares Node HMAC with the existing native crypto implementation.
The native witness asserts ordered values, UTF-16, numeric bits/NaN, genuine
Error versus plain object, original caller/error/info/client/factory handles,
effects and final state before encoding. It covers ordinary own properties,
dense arrays, primitive/ASCII fixtures and bounded controlled awaits. General
accessor/proxy/symbol/prototype behavior, arbitrary coercions/iterators,
JSON/query codec completeness, GC/general promise scheduling and live network
behavior remain outside this floor. Imported bodies and Store operations are
controlled dependencies; no concrete complete SDK/Store/HTTP/socket/signer
assembly is claimed. Production balance-retention is a separate scoped gate.

## Bitrue exchange runtime and position bodies

`safe-cex-venue-bitrue/src/runtime_lifecycle.rs` ports nine exchange bodies:
dispose, queueFastPrivateRefresh, runFastPrivateRefresh, validateAccount,
start, tick, parsePositions, listenOHLCV and listenOrderBook. LifecycleHost
extends the existing venue-local ReadHost with original global/superclass,
Set snapshot/add/clear/size, dynamic property and array-find bindings. Current
underlying helpers/HTTP/socket/Store methods, native timer/loop/promise behavior
and primitive/array/spread operations are dependencies; no shared SDK trait
changes or concrete full exchange/Store/HTTP/socket assembly are claimed.
Class construction and the four public websocket rows remain pending.

Disposal preserves truthy timer clear/reread/reset, original fill-listener
identity, super-before-current-socket order and original exceptions. Debouncing
uses350ms, retains the original timeout result after successful invocation,
and resets it before resolving the current refresh member. Its callback returns
void. A linked timer case actually invokes the new run_refresh body. Refresh
captures and clears insertion-ordered pending symbols before awaiting orders;
late arrivals requeue in finally. Running resets and pending requeue still occur
after disposal or logger failure; a distinct finally error overrides the
original request failure. Native state borrows end before any callback/await.

Validation follows the source then/catch chain: rejected balance or failed
balance property projection becomes undefined; a synchronous fetch invocation
failure still escapes. Startup captures public-only mode after external-data
initialization and before the market await. It preserves13 ticker fields,11 initialized to zero,
original markets/account/orders, current loaded spreads, unawaited connections/
tick and post-await disposal guards. Tick uses strict numeric contracts0,
refreshes symbols of newly absent/closed positions in old-position order,
updates Store before order refresh, reports errors then conditionally schedules
the imported loop. Its callback returns the current tick member's original
result, including replacement during the preceding await.

Position parsing calls both account/snapshot helpers before its fallback test;
falsy inputs return the original current Store positions handle. Metadata
receives original snapshots, then mapping reads the retained mutable accounts.
Nested maps/flatten retain raw entry/contracts/notional fields and original
info, strict numeric positionType1, truthy metadata fallbacks, source position
IDs, current existing liquidation/ticker reads, signed pnl and first positive
finite liquidation candidate before existing-value/zero fallback. Already-ported
helper/metadata divergence classifications remain; original underlying helper
bindings define this body rather than a new claim about those adapters.
The two exchange subscription wrappers retain original arguments, callbacks,
cleanup returns and exceptions. Candles add even an empty symbol to priority;
book delegation adds no priority effect.

The actual unchanged source/native witness has nine tests and independent
finite floors disposal7, queue8, refresh10, validation7, start10, tick9,
positions9, candles3 and book2 (65 cases). The floors end respectively at
replace-private, linked-refresh, empty-pending, balance-sync-throw,
private-throws, current-loop-member, after-await-mutation, delegate-throws and
delegate-throws. Four SDK files are hashed; source arrows/enums/constants and
position-ID helper run in the oracle. Native ordered values, UTF-16, number
bits/NaN, original error/caller/position/snapshot/info/cleanup handles,
effects, callbacks and final state are asserted before encoding. Ordinary own
properties/dense arrays, primitive/ASCII fixtures and bounded controlled
promise/timer schedules define this floor. Proxy/accessor/prototype/symbol
behavior, arbitrary coercions/iterators, general GC/promise/loop scheduling,
network and full SDK assembly are outside it. Store methods are controlled in
the witness; production balance-retention is a separate scoped regression.

## Bitrue class composition and public subscriptions

The six remaining rows form three units: exchange/public class composition and
constructor wiring in composition.rs; Blob/message routing and retained candle/
book subscriptions in public_streams.rs. CompositionHost/StreamHost extend only
the existing venue-local ReadHost. They expose original superclass/import/ABI
and ordinary native property/callee/array/Set/Map/JSON/Blob/timer operations.
No shared SDK type/trait changes or fixed complete SDK assembly are claimed.

The exchange factory forwards the explicit constructor's three slots to super
(padding absent values and ignoring excess arguments), initializes14 data fields
and46 exact existing member ABIs in source order, then invokes constructor_body.
Client-ID rule import gets Bitrue and /^.{1,31}$/, Grafana arrays stay original
module aliases, and mutable containers/functions are fresh per instance.
Secondary-key construction retains original opts/API/limiter, maxRPS5, original
limited/unlimited clients, original owner for public/private constructors, and
original fill callback after both sockets. Public's implicit constructor forwards
all arguments, creates four fresh data containers and16 exact member ABIs;
detached native ping is witnessed. The member provider is an explicit original
ABI boundary, with source name and lexical receiver. It binds every existing
body, while keeping prior leaf-helper divergences; all-member invocation,
base/runtime implementations and whole SDK assembly are outside this floor.

Message routing destructures before disposal, tracks before Blob testing, awaits
the original decompression import then rechecks disposal, parses through the
original JSON helper and preserves falsy/unknown-data and ping precedence.
Resolvers receive original parsed frames. Partial failure leaves requests;
reentry uses the current request table/channel when clearing after fanout.
Handlers receive current tick||data. Parser/decompression/forEach implementations
and promise scheduling remain dependencies rather than new complete codec ports.

Candle subscription retains topic/parser/caller callback and current mapKline,
appends via source spread, increments with JS+ and sends subscription. Recursive
100ms waits survive cleanup: cleanup-before-connect may later subscribe, exactly
as source. Cleanup filters only the original parser, decrements even before
subscription (NaN possible), and conditionally unsubscribes when connected with
an empty handler list. Repeated cleanup and foreign handlers remain observable.

Book subscription retains mutable timer/received state, subscribed Set, parser
dictionary and one book object. Both converters run; primary subscription starts
a3000ms fallback when no update arrives and resets best prices0. Received becomes
true before statistics/parsing, inhibiting fallback even if parsing fails.
Mapping preserves buys||bids/asks, raw price/amount fields and original totals
binding before Store/callback. Cleanup removes only owned functions, deletes
parser entries, clears the same book asks then bids, cancels truthy timers, clamps
source decrements and conditionally unsubscribes. Old callback book aliases see
the clear. Source string totals/count coercions are preserved.

The actual-source/native witness has six tests and independent finite floors
exchange-class4, constructor6, public-class4, message15, candles8 and book13
(50 cases). They end at map-throws, listener-throws, set-throws, handler-throws,
callback-throws and string-amounts. Seven actual SDK source files are hashed;
unchanged classes/arrows/constants/client-ID/JSON/totals helpers run in the
oracle, with Blob decompression as an original controlled import. Native ordered
values, UTF-16, numeric bits/NaN, original errors/rule/Grafana/frame/book/function
handles, callback results and final state are asserted before encoding. Registry
targets have independent46/16 expectations. Actual waiting and cleanup-before-
connect schedules are separate; original returned cleanup survives later callback
failure. Ordinary own properties/dense arrays, primitive/ASCII fixtures and
bounded controlled timer/promise schedules define the floor. Arbitrary proxy/
accessor/prototype/symbol/coercion/iterator/GC/promise behavior, full JSON/gzip
codecs, general event-loop scheduling and concrete full SDK/Store/HTTP/socket
assembly remain outside it. Production balance-retention is a separate gate.

<a id="exchange-runtime-callback-choice"></a>

### Exchange callback delivery and owned continuations

Native callbacks use fallible ExchangeError results and the required observable
report_exception host operation for failures outside an awaited call. This keeps
the original failure on one explicit host channel instead of unwinding through
an executor or foreign callback. It loses JavaScript arbitrary thrown-value
identity and Promise/uncaught-exception scheduling. Recording hosts establish
routing, not delivery by production hosts absent from this tree.

Signer registration and the first income-page callback start at invocation.
Returned futures own their pending continuation: dropping a signer observer
unsubscribes, and dropping a pager drops its current fetch. This deliberately
bounds abandoned native resources rather than retaining work until eventual
availability or Promise settlement. Source discarded Promise observers do not
cancel those effects. The eager prefix does not make an arbitrary host's lazy
transport future execute before polling. Resync fetch starts eagerly, while a
yield prevents a ready result's reactions from running on the initiating stack;
this does not emulate the full JavaScript microtask queue.

Reconnect retains the generation-bound provisional Open bridge so an inline
native dial cannot lose its account reconnect event. Source installs its Open
listener after the connect hook, so inline Open observation differs. Native
console diagnostics flatten the options object rather than exposing credentials
and live inspection objects. The selected WsSocket close/reconnect ABI reports
transport failures through socket events rather than a second synchronous return
channel; source custom-host throws and their short-circuit effects are excluded.
Required WsParent/WsRuntime capabilities and typed errors are choices under
[§15](#closed-typed-output-boundary), not Rust language limitations. Recovery's
explicit subclass-policy boolean and positive opaque timer identities retain
the existing native lifecycle protocol instead of importing mutable subclass
reflection and JavaScript numeric timer truthiness.

Dormant socket activation owns one pending inner connection. Reopening again
cancels that operation; host discard, close and disposal release it. This bounds
native resources rather than reproducing RWS's overlapping zero-delay chains:
two reconnect calls before either timer runs can construct two source sockets.
The base reconnect's outer delay remains independent, including its source
force-close race. Deferred construction failures use the existing native async
error channel; RWS's generic constructor-chain catch only releases its lock.
These are CHOSEN ownership/error-delivery costs, not language limitations.

<a id="public-data-provider-choice"></a>

### Public-data provider model and host ownership

Provider bodies retain supplied socket, parent, timer and source capabilities.
Keeping those capabilities explicit permits native and wasm assembly without
embedding browser globals or a second exchange implementation. It excludes
arbitrary JavaScript receivers/accessors and thrown values; errors use the shared
fallible protocol. Weak callback captures bound abandoned provider resources,
at the cost of callbacks no longer retaining a dropped provider. Array handles
and the in-process loaded-flags handle retain live identity; typed records and
callback payloads follow [§16](#owned-snapshot-vs-live-alias). Cross-array item
identity and arbitrary callback mutation are not reproduced. A cloning Store
adapter would break delta aliasing, so none is supplied; concrete facade/host
assembly is not established by these body tests.

GatewayMessage's public decoder retains a closed, fully validated envelope for
typed outbound construction and consumers, following
[§15](#closed-typed-output-boundary). It can reject a whole frame for an unused
malformed field or optional null, and discards unknown top-level properties.
The provider's on_message path instead captures retained JSON and decodes only
the selected channel payload. Its refusal of malformed typed payload records
avoids passing inconsistent domain records to Store/callback consumers.
String-only error/status logging retains the declared gateway log-field contract
without running arbitrary value inspection/coercion while reporting a failure;
nonstring log fields therefore lose their source coercion text. Unchecked source
casts, coercion effects and malformed records are excluded. These are distinct
boundaries. The shared retained-capture depth budget can
also refuse a whole incoming frame for a deep unused field before channel
dispatch; retaining the common bounded decoder avoids an unbounded parallel
parser solely for ignored data. The repaired literal own-key and numeric
ingress paths are not exemptions to
[§17](#numeric-ingress-defect).

GatewayChannel explicitly admits the source's emitted auth tag rather than
retaining a union which contradicts the running source. The candle-key parser
keeps its scalar fallback for malformed no-colon keys: widening its native
return merely for unreachable generated-key inputs would add another string
model at an otherwise valid delimiter boundary. No actual malformed-key source
producer or FORCED surrogate observation is claimed. Valid generated keys,
including astral symbols and colons, remain exact.

<a id="shared-handler-and-ledger-choice"></a>

### Handler registries and typed income ledgers

MessageHandlers retains the single-frame Value/ExchangeError callback protocol
used by native dispatch. Adding a parallel variadic executable-object protocol
would separate callback/error ownership from all existing dispatch consumers;
extra positional arguments and raw JavaScript thrown values are excluded.
IndexMap preserves registration order but does not reproduce JavaScript's
integer-key enumeration priority. Shared Rc lists and callable identity are
retained. MessageHandlersStats keeps the current concrete record and
PrototypeScalar count model so diagnostics and reconciliation share one typed
view. Arbitrary replacement record shapes and executable property access remain
outside [§15](#closed-typed-output-boundary); current configured prototype-realm
behavior is described at [the stats boundary](#diagnostic-prototype-write-choice).

Income fetches/results use the shared IncomeRecord ledger instead of a second
dynamic ledger whose values existing account consumers cannot consistently
read. Nullish/coercible/open records and executable getters are excluded under
[§15](#closed-typed-output-boundary); returned records follow
[§16](#owned-snapshot-vs-live-alias). The fixed fill-unit table uses symbolic
ProtoMember identities rather than exporting executable Object.prototype
functions, avoiding a second JavaScript callable/prototype runtime solely for
table misses. Builtin membership/arity are retained; custom prototype mutation,
callable execution and shared mutable prototype identity are excluded. The
unmodified table has no retained-surrogate observation.

<a id="registry-loader-choice"></a>

## Registry constructor, object and continuation choices

The registry adapter deliberately exposes typed Rust constructor capabilities
through explicit module hosts. A generic export-value/thenable/throw carrier is
feasible; choosing it would extend the native caller contract beyond invoking a
constructor factory and add JavaScript adoption semantics to that native
public contract. We keep those platform decisions in the module host instead, so the
registry hands native callers the existing typed construction operation. The
price is that scalar/object exports and thenables have no native return
variant. Errors require Rc<dyn Error>; a host can wrap an arbitrary source
throw, but the loader supplies no raw-throw carrier or runtime export validator.
Missing exports remain None, and admitted Rc errors retain identity. The
constructor capability exposes construction, not JavaScript class static fields
or prototype reflection. This is CHOSEN, never a Rust impossibility claim.

The source registry is a mutable ordinary object; the native descriptor and
callable tables are fixed ordered slices. We choose a stable compiled route
inventory rather than a mutable route-registration service with property and
prototype semantics. The descriptor projection permits listing routes without
calling module hosts, while the callable projection retains delayed loading.
Their duplicated route literals must be checked against the same maintained
source inventory. The price includes inability to replace a loader, add an
ordinary property or observe/change the registry object's prototype through
these native tables. A mutable host object could mirror those operations.

Web import begins when a loader is called, but export continuation belongs to
the returned native future. We choose caller-owned progress and cancellation
instead of retaining hidden executor jobs after a caller discards its result;
full Promise-style continuation could be supplied through an explicit host
scheduler. Dropping a pending or ready-but-unpolled future can therefore omit
export lookup, while a discarded source Promise continues. Native require and
export lookup stay synchronous before Ready exists; later future polling still
uses the caller's executor rather than JavaScript's microtask queue.

These are adapter choices, not proof that concrete venue module hosts,
constructors or facade assembly have been supplied. Source/native loader
witnesses cover the declared finite routes and selected scheduling/value costs;
they do not establish general JavaScript object or event-loop emulation.

**Scope amendment (2026-10-16).** One exception now applies to the paragraph
above: the names in `safe-cex-registry/src/rust_venues.rs` (`backpack`,
`binance`) have concrete, statically linked Rust constructors. A caller reaches
them only by choosing `ConstructionPath::Rust` for that name. They use the same
typed `ExchangeConstructor` capability, so none of the three choices above
changes. The default `ConstructionPath::ModuleHost` keeps every module-host route
and its price. That is why the `exchangeRegistry.<name>` rows stay divergent and
claim the Rust route as split members. `Rust` for any other name is a named
error, never a fallback. The design is in `safe-cex-registry/CONSTRUCTION.md`.

<a id="utility-capability-choice"></a>
### Utility capability choices

CHOSEN: these utility adapters keep platform discovery and I/O ownership with
the embedding instead of adding a JavaScript global-object and background
executor protocol. Mirroring that protocol is feasible. The following local
reasons and prices apply only to the named functions; they do not excuse
differences beyond these listed costs or establish concrete host assembly.

- `uuid` uses native CSPRNG filling without another JavaScript entropy-provider
  pool. It keeps the installed URL alphabet, mask, forward traversal and removal
  rule, but changes entropy draw batching and returns string errors through
  Result instead of source throws. Equal random outcomes are not claimed.
- `isNodeOrBunRuntime` accepts a materialized process snapshot so the embedding
  can identify a JavaScript-compatible runtime without guessing from the native
  process. Live globals, inherited/getter properties and a null-process throw
  are omitted. `isDevelopment` accepts the startup NODE_ENV for the same reason;
  its helper does not cache results. The embedding must capture it once to match
  the source module's stable initialization value.
- `isSigningCaptureRequest` reads an own data marker so property execution does
  not implicitly control HTTP accounting. Inherited/accessor markers are outside
  that ABI; own JSON truthiness remains required. `isUnknownOutcome` classifies
  materialized error properties without executing another error-object protocol
  during placement recovery. Getter/prototype/executable coercion and identity
  are omitted, and coercion failure is a string Result instead of a thrown
  object. JSON admission does not imply every numeric intermediate is finite.
- `readBlob` keeps acquisition with the platform and decompression with the byte
  utility, avoiding another Blob receiver/accessor protocol. The host supplies
  the byte future; native progress requires polling and dropping it can cancel
  continuation. Source invokes arrayBuffer synchronously and retains its Promise
  continuation. The host owns the acquisition prefix; this utility alone does
  not reproduce source call-entry or cancellation effects. Blob receiver and
  prototype effects are outside the byte-future ABI; supplied-future and inflate
  failures use string Results instead of preserving pako's thrown error values.
- `bybitTimeUrl` maps into the clock's venue-time host capability so proxy
  selection, HTTP and date parsing share the embedding's transport instead of
  adding another clock-owned client. Native code does not construct/import the
  source URL and cannot establish its proxy or 5000ms timeout binding.
  `scheduleNextSync` similarly leaves timer execution/cancellation with that
  host instead of creating an implicit executor. Host future progress and handle
  representation replace JavaScript callbacks, including no automatic
  unref/process-exit behavior. The source 90000ms threshold with a 300000ms period
  remains a preserved ts-bug. Browser/worker integration must install real hooks;
  the default native timer hook does not schedule background work.
- `virtualClock` uses one lazy shared native static rather than allocating on
  source module evaluation. Its fixed native runtime default replaces source
  runtime discovery during later synchronization, not during construction.
  Separate configured browser/worker instances and their wiring remain the
  embedding's work.

Other string, output and alias costs remain governed by
[§14](#shared-utf8-string-boundary), [§15](#closed-typed-output-boundary), and
[§16](#owned-snapshot-vs-live-alias); numeric repairs and classification use
[§17](#numeric-ingress-defect), [§18](#divergence-taxonomy), and
[§19](#forced-surrogate-conflict).

<a id="secondary-key-policy-choice"></a>
### Secondary-key eligibility policy

CHOSEN: `exchangesWithoutSecondaryKey` and `exchangeSupportsSecondaryKey` use one
immutable compiled exclusion list so callers cannot silently alter credential
eligibility through a shared exported array. The initial fifteen names and exact
membership are retained. Source array identity and later push/splice changes
remain observable costs; adding a synchronized mutable eligibility service is
feasible but deliberately outside this fixed policy interface.

<a id="diagnostic-adapter-choice"></a>
### Diagnostic config and auth adapters

CHOSEN: isAuthOperationObject reads a materialized JSON object so redaction does
not execute an additional property-access protocol. readConfigNumber and the
four config stamp functions use a typed own-field config for the same reason:
request bookkeeping should not implicitly execute arbitrary setters. This ABI
omits inherited fields, general accessors, functions, cycles and descriptors;
it cannot distinguish a missing modeled number from a present undefined or
wrong-typed property. The optional proxy stamp similarly collapses missing and
present undefined/null values; its sealed presence test cannot recover that
distinction. Request-start/signing callers supply now instead of invoking the
source Date.now default; they own matching that lookup. A fuller object protocol is feasible. Native f64 values
remain admitted, and modeled sealed existing fields remain writable. Freeze,
nonwritable properties and arbitrary whole-config mutation from a getter are
outside that sealing flag. Direct signing retains ordered partial writes and
its string Result error rather than returning the source config or thrown value;
the public wrapper retains the source catch behavior.

proxyHeaderOf keeps the typed config and header capability instead of embedding
Axios object reflection. It retains the supplied bound callback and its failure,
but does not emulate dynamic replacement of the get property between the
source typeof check and call. Prototype/enumeration/coercion side effects and
original thrown-object identity are omitted. Scalar own header values still
follow the shared conversions. These choices do not prove any concrete native
HTTP, signer or proxy binding.

<a id="diagnostic-percentile-choice"></a>
### Diagnostic percentile sorting

CHOSEN: percentile uses stable insertion sort to admit native NaN values without
requiring a total-order comparator or embedding a particular JavaScript engine's
sort. A versioned engine-sort implementation is feasible but is deliberately
outside this portable diagnostic aggregate. The price is a possible different
permutation when NaN makes the comparator inconsistent. Finite ordering, index
selection, empty input and numeric rounding remain required behavior.

<a id="merge-reader-choice"></a>
### Timestamp merge capabilities

CHOSEN: mergeUpdatesUsingLastUpdate accepts clock, cutoff and typed field readers
so it can merge native row types while retaining their Rc identity, without
adding another row-reflection and virtual-clock ownership service. Callers must
supply matching clock/cutoff inputs; default arguments and source global clock
lookup are not performed here. Readers may have effects: repeated lastUpdate
reads, left-to-right ID lookup and eager mapping before deduplication remain
required. Readers return native &str/Option<f64> rather than a JavaScript
property/throw protocol; arbitrary thrown values, general property coercion
and live array/prototype mutation are omitted. This interface does not establish production venue assembly.

<a id="woo-public-canonicalizer-bodies"></a>
## WOO public canonicalizer bodies

Canonical declaration: **`woo-public-canonicalizer-bodies`**.

`safe-cex-venue-woo/src/public_canonical.rs` implements canonicalString,
canonicalMarket, canonicalLegacyTickerStats, canonicalV3TickerStats,
canonicalOrderBookLevel, canonicalCandle and the three market/ticker list
canonicalizers as native bodies over required venue-local CanonicalOps capabilities.
CanonicalModule owns the persistent native mapper handles.
The host carries original opaque JS values: cloning retains identity/live
contents, including arbitrary UTF-16, undefined, nonfinite numbers, BigInt,
callables and compound/prototype values. No owned JSON/UTF-8 ingress restriction
is imposed by these bodies. Object construction creates ordered own data
properties with original field handles. Host errors remain unchanged.

The Rust bodies own the exact property projections, renamed V3 fields,
repeated precision/limits reads, and the symbol comparator. They delegate
source operations rather than complete canonicalizers: current JSON.stringify,
iterable spread, current sort/map methods and localeCompare lookup/invocation.
Sort invokes the native comparator with source callback/coercion semantics;
map invokes the native projector, preserving source holes/iteration/effects.
Map receives the actual sort return, including overridden non-array returns.
The comparator captures a.symbol and its localeCompare member/receiver before
reading b.symbol. Preparation resolves the member only; noncallability is
rejected at invocation after the RHS read. Native callback handles can
be retained by overridden sort/map and invoked after canonicalization returns. JSON.stringify returns an opaque value, preserving even its
undefined result or overridden result and original errors. These mandatory
host contracts are dependencies, not a claim of a concrete complete runtime
host/SDK assembly or of all possible host implementations.

The actual-source/native witness stops at five projectors, three sorted-list
families (three rows each, OTHER/42/OTHER with stable equal keys), live compound
alias and U+D800 retention, two precision getter reads, one throwing getter,
five stringify cases (undefined/NaN/-0/U+D800/BigInt), one replaced
localeCompare member with original sort error, one non-array sort result, and retained comparator/projector calls after
canonicalization returns.
Decoded/prepared native properties, units, bits, identities, access order,
errors and sorted IDs are asserted before secondary wire comparisons. Fixture
built-ins provide controlled ordinary object/array behavior and pinned root
collation for the ASCII symbols; they do not prove complete JavaScript built-in
or runtime-host implementation. No new concrete diagnostic carrier inherits
restrictions from the earlier public_parity module.

<a id="mexc-native-socket-owner-choice"></a>

### MEXC native socket construction

The concrete MEXC factory accepts an existing BaseExchange, configured REST
adapter, local-time conversion, WebSocket runtime, remote account name and typed
fill sink. It constructs and registers both native socket protocols against that
same exchange and Store. Supplying these shared capabilities explicitly is
CHOSEN under [§18](#divergence-taxonomy): it avoids a second implicit host factory
inside the venue. The call shape differs from the source parent-only socket
constructors, and the factory does not recreate createXhrWithSecondaryKey.
The generic AssemblyHost constructor remains a separate callable body, not an
implementation of these concrete capabilities.

Native sockets retain weak parent references and own their timers and pending
callbacks through the existing core registries. A dropped parent or disposed
owner cannot continue updating its Store through this binding. That resource
bound, typed fill/error callbacks, asynchronous dial installation and explicit
host exception reporting follow
[exchange-runtime-callback-choice](#exchange-runtime-callback-choice), rather
than reproducing arbitrary JavaScript callback values or the complete Promise
scheduler. Public candle/book sinks retain the shared infallible callback signatures and
owned model values. Source callback throws cannot be returned as ExchangeError
through these signatures; mutating a delivered book cannot change the stored
native book or another subscriber view. These are CHOSEN callback and snapshot
costs under [§15](#closed-typed-output-boundary) and
[§16](#owned-snapshot-vs-live-alias), avoiding a venue-only callback/error/model
API alongside the common Exchange interface. Cleanup uses its one-shot native
ListenerCleanup, so repeated source cleanup invocation is unavailable. The
source snapshot-only extra lastUpdateId is unavailable through the closed
OrderBook callback model. Private balance-refresh failures are observable through the host
reporter where source silently catches them; this deliberate extra observation
prevents a failed refresh from appearing successful.

<a id="mexc-borrowed-method-choice"></a>

### MEXC borrowed method futures

The concrete MEXC account/market/order methods keep the shared borrowed
ExchangeFuture interface. This is CHOSEN under [§18](#divergence-taxonomy) to
retain caller-owned borrowing and cancellation instead of introducing a second
venue-local owner for unobserved Promises. Constructing and dropping an unpolled
future omits the source eager child/HTTP call. Once polled, dropping the future
can omit later normalization, reporting and Store updates. Native host adapter
operations that have already started may survive independently; this is not a
claim that dropping a future cancels a transmitted request. Owned socket tasks
have their separate core owner and do not inherit observer-drop cancellation.

<a id="mexc-currency-own-record-choice"></a>

### MEXC currency dictionary projection

MEXC keeps homogeneous own JsObjectMap<Balance> currency records. This is CHOSEN
under [§18](#divergence-taxonomy) rather than widening every native currency
lookup to a Balance-or-inherited-primitive union. Source assignment to __proto__
can expose inherited free/used/total/upnl numbers that are not currency entries;
those inherited numeric reads are unavailable in the native map. Ordinary own
currency records and JavaScript own-key ordering remain supported. The separate
info snapshot cost cites [§16](#owned-snapshot-vs-live-alias). This declaration is
not supplied by the diagnostic prototype-write policy, and does not call the
source prototype behavior repaired.

<a id="mexc-report-input-choice"></a>

### MEXC raw reporting projection

MEXC selects admitted JSON response detail and hands the selected value to the
shared native reporter as ExchangeErrorInput::Raw. Strings take its Message
path and falsy values its Missing path; truthy objects/numbers reach the host as
the original JSON snapshot, with only formatter metadata (message, name,
context, code, url, method) projected from it. AccountValidationError separately
retains the selected JSON value without String coercion. This is CHOSEN under
[§18](#divergence-taxonomy): reuse ExchangeError and the required host reporting
contract instead of adding arbitrary JavaScript thrown graphs to every sink.
The raw console/Sentry object identity, executable getters and custom prototypes
are unavailable. A non-string nested error.message is projected to a native
String, so its original emitted value type is also lost. These costs are
separate from the owned response snapshot at
[§16](#owned-snapshot-vs-live-alias) and string ingress at
[§14](#shared-utf8-string-boundary); they are not language limitations.
