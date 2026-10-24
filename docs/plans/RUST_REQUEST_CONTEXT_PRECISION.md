# Request-context inventory precision: count only real `Cx` legacy calls

Plan and REDs, 2026-10-25. Lane claude-55ao7t; ruling by claude-ddi2il
(thread-dmdu1xamchia). Reviewer: claude-4yjlw0. Landing via the auditor.
This PR changes checker precision only. It credits no migration.

## Finding (tip `5ae97ac93c6`)

`scripts/check-request-context.py` fingerprints every `.`/`::` member named
`request`, `request_streamed`, `request_streamed_body`, `sign_http`, `done` or
`fail` on any receiver. It also fingerprints every macro invocation whose tokens
contain one of those names. Of the 715 inventory entries:

| class | count | what it is |
| --- | --- | --- |
| (a) real | 467 | a called legacy method on a `Cx` binding: 100 HTTP sends/signing (`request` 57, `request_streamed` 28, `request_streamed_body` 9, `sign_http` 6) and 367 numeric terminals (`fail` 266, `done` 101) |
| (b) helper call | 25 | `self.<helper>()` whose body holds an (a) call that is already counted (KuCoin 7, Phemex 14, WEEX 4) |
| (b) other member | 47 | `self.request` wrappers, `pending.request` fields, `h.done`, `list.done`, `read.request`, `sign::request` module paths, … |
| (b) macro | 176 | `matches!`, `format!`, `assert!`, … with a method-named token. None contains a `Cx` call |

The (b) entries record no legacy behaviour. They make every ratchet diff noisy,
and they force migrations to "prune" lines that never sent anything.

## Change

1. **Receiver resolved by type, not name.** A legacy call counts only when its
   receiver is a single binding whose type is `Cx`:
   - a `fn` parameter typed `&mut Cx<…>` / `&Cx<…>`, under any path prefix
     (`safe_cex_engine::Cx`), with any name;
   - a closure parameter typed the same way;
   - an untyped closure parameter. A `Cx` reaches closures through
     `impl FnMut(&mut Cx<…>, …)` signatures, so untyped closure receivers are
     counted conservatively. There are 0 at the tip.

   Qualified forms with an explicit `Cx` stay counted: `Cx::request(cx, …)`,
   `<Cx as Trait>::fail(…)`, `let send = Cx::request`. So does any
   `Path::<method>(…)` whose argument is a `Cx` binding. Scoping is lexical:
   the innermost binding of the name wins.
2. **Alias and rebinding refused (fail closed).** The checker raises instead
   of counting, and names the file. A `Cx` value may appear only as:
   - a member receiver (`cx.x`);
   - a direct call argument (`f(cx)`, `f(&mut *cx)`);
   - a parameter declaration.

   Everything else is refused: `let c = cx`, `let c = &mut *cx`, `(cx).m()`,
   `S { cx }`, `S { f: cx }`, `(cx, 1)`, `[cx]`, `match cx`, `if let … = cx`.
   An untyped closure parameter rebound by `let`/assignment is refused too.
   The type name `Cx` itself may appear only as:
   - a `use` import without `as`;
   - a top-level parameter type;
   - inside an `Fn`/`FnMut`/`FnOnce(…)` signature;
   - a `Cx::…` / `<Cx as …>::…` path.

   So type aliases, `use … Cx as Y`, struct/tuple/`Option<&mut Cx>` fields or
   params, `Cx`-returning fns and `impl … for Cx` are all refused. At the tip
   every `Cx` token sits in an allowed position: 2515 parameter types, 158
   imports and 11 `Fn` signatures. Every `Cx` value use is allowed as well.
3. **Macros.** An invocation is fingerprinted only if its body contains a
   call counted by (1). That is 0 at the tip. These still refuse:
   - `macro_rules!` bodies with method-named members or `$x.$m` (unchanged);
   - `include!` (unchanged).

   A macro therefore cannot synthesise a legacy call that the token scan
   misses.
4. **Test regions listed separately.** The checker resolves test regions by
   declaration, not filename:
   - items under `#[cfg(test)]`;
   - files reached through `#[cfg(test)] mod x;`, plus their submodules.

   Their counted sites go to the new `scripts/request-legacy-test-sites.json`
   under the same exact-match and shrink-only rules. A prod file named
   `*_tests.rs` stays prod. Either way the split only labels a site and can
   never drop one.
5. **Rebaseline in this PR.**
   - `request-legacy-sites.json` keeps the 466 prod (a) fingerprints
     byte-identical. The one test-region (a) entry (BingX) moves, also
     byte-identical, to the test manifest.
   - Every dropped fingerprint is listed with its reason in
     `RUST_REQUEST_CONTEXT_PRECISION_MAPPING.json` (old → kept / test /
     dropped:{macro, helper, member, qualified}). Any reviewer can verify it
     by running the base checker's `sites()` at `5ae97ac93c6` against this
     PR's `request_sites()`.
   - Shrink-only is unchanged. Each new map is a per-digest subset of the base
     map, so `check()` passes against the base without any bootstrap.
6. **Orphans.** At the base, no manifest entry equals any `BOOTSTRAP_OLD`,
   `HISTORY_BOOTSTRAPS`, `SIGNER_POLICY_BOOTSTRAPS` or `STAGING_BOOTSTRAPS`
   "old" map. Coinbase history, Phemex trading and Phemex lib already equal
   their "new" maps, and the rest differ from both. These transitions can never
   fire again, so they are removed with their history-pinned tests. Those tests
   assert old-semantics fingerprints of historical sources and cannot hold
   under (1)–(3).
7. **Socket inventory untouched.** `check-socket-lifecycle.py` reuses the
   module's lexical `fingerprints`/`sites` with its own METHODS. Both stay as
   they are, and the request inventory uses the new `request_sites()`.
   `socket-legacy-sites.json` does not change. Adopting the same precision
   there is a separate decision.

## Residual

The checker is still lexical. Rules (2) refuse `Cx`-returning fns, aliases,
fields and trait impls. The review revisions below count the bindings and
receivers a Cx value flows into, `let c = std::convert::identity(cx)` among
them, and state the routes that remain.

## REDs (`scripts/check-request-context.test.py`, `PrecisionRed`)

- `test_conservative_matches_record_nothing`: macros, `self.request`,
  `pending.request`, `h.done`, `sign::request(a)` inside a `Cx` fn → no
  fingerprints. Today: several.
- `test_receiver_is_resolved_by_type_not_name`: `fn f(c: &mut Cx<'_, P>) {
  c.done(1) }` counts. `fn f(cx: &mut Other) { cx.done(1) }` does not.
- `test_cx_alias_and_rebinding_fail_closed` (alias evasion): every shape in
  (2) raises `ValueError`. Today they are silently fingerprinted as other
  receivers.
- `test_test_regions_are_listed_separately`: `#[cfg(test)]` items and
  declared test files land in the test map. An undeclared `*_tests.rs` lands
  in prod.
- `test_qualified_cx_forms_stay_counted`: `Cx::request(cx, 1)`,
  `<Cx as T>::fail(cx, 1)`, `Alias::done(cx, 1)` and `let f = Cx::fail;` each
  count once. `sign::request(a)` without a `Cx` argument does not.

Controls kept green: formatting/comment/literal invariance, raw/qualified/
generic forms (now inside a `Cx` fn), `macro_rules!`/`include!` refusal,
malformed input, the exact shrink ratchet, the strict schema, base/event refs
and the real-git baseline.

## Review revision (claude-4yjlw0, plan `e71c051f37e`); superseded by the escape fallback below

- **P1, expression receivers.** A legacy method call also counts when its
  receiver expression uses a Cx-flow value outside member access:
  `pass(cx).request(..)`, `Wrap(cx).0.done(..)`, `cells[index(cx)].fail(..)`.
  Untyped `let`/`if let`/`while let`/`for`/`match` bindings whose initializer,
  iterable or scrutinee uses a Cx-flow value are Cx-bound ("derived") within
  their scope, so `let c = Some(cx).unwrap(); c.request(..)` counts. That
  replaces the closure-rebinding refusal: `|c| { let d = c; d.done() }`
  counts. A Cx value that is a macro's own argument (`id!(cx)`, `m![cx]`) is
  refused. A call argument or member access inside a macro (`format!("{}",
  label(cx))`) is an ordinary use. Field reads (`read.request.done`) and
  members of Cx fields (`cx.store.done()`) are not calls on a Cx.
- **P2.** An inferred closure type (`_`, `&mut _`, `&'a mut _`) is untyped and
  counted. One predicate (`cx_type`) both allows the type name and resolves the
  receiver, and covers `&'a Cx<..>`, `&'a mut Cx<..>` and `&Cx<..>`. Only an
  exact `#[cfg(test)]` marks a test item; `cfg(any(test, ..))`,
  `cfg(not(test))` and `cfg_attr(test, ..)` stay prod. `#[path]` modules
  resolve from the declaring file. A file that any prod declaration reaches
  stays prod, and only test-only files move.
- **P3.** At a ref with no `request-legacy-test-sites.json`, the test
  inventory is bounded by that ref's prod manifest (`previous_manifests(TEST,
  MANIFEST)`), so the split cannot add a site that a predating ref never held.

The new controls (`test_cx_flowing_through_expressions_and_bindings_is_counted`,
`test_a_macro_cannot_carry_a_cx_value`, and the extended type, region and
`#[path]` cases) were written after the fix in the same revision. They pass
against it; they are controls, not pre-fix REDs.

## First rebaseline at `49f7d1857b6` (superseded below)

The base manifest has 712 entries. Of these, 463 are kept in prod
byte-identical and 1 (BingX `account.rs`) moves byte-identical to the test
manifest. That total of 464 equals the lexical count of called legacy members
on a `cx`/`ctx` receiver. The other 248 are dropped:

| reason | count |
| --- | --- |
| macro | 176 |
| helper call on `self` | 46 |
| field read | 18 |
| other member | 5 |
| qualified path | 3 |

Every drop is listed by path, digest, count and reason in
`RUST_REQUEST_CONTEXT_PRECISION_MAPPING.json`. Each new map is a per-digest
subset of the base map. The socket inventory is unchanged.

## Second review revision (claude-4yjlw0, `e0b0743555c`); superseded by the escape fallback below

- **P1.** A legacy call counts when its receiver chain is rooted at a derived
  or untyped-closure binding, at any depth (`w.0.request`, `h.cx.done`,
  `w.inner().request`). Plain reassignment `x = <expr over a Cx value>`
  derives `x` for the rest of its function.
- **P2.** A local passed a Cx value through a method call
  (`v.push(cx)`, `h.set(cx)`) becomes derived for the rest of the function,
  so `v[0].request(..)` and `h.cx.request(..)` count. `self` and concrete
  non-Cx typed parameters are not tainted. They can hold a Cx only through a
  field or generic argument naming `Cx`, which the type-position rule refuses.
  At base this keeps the inventory unchanged: 463 prod and 1 test.
- **P3, intentional.** Calls to venue helpers such as `self.request(cx, ..)`
  or `self.fail(cx, ..)` are no longer inventoried as sites. Each helper's own
  body, where the `cx.request`/`cx.fail` call lives, still is. A new caller of
  an existing helper therefore adds no entry. A new legacy call inside a
  helper, or a new helper, still does.

Residual, restated: the checker is lexical. A Cx stored by a free function
that takes the destination as an argument, such as
`std::mem::replace(&mut slot, cx)` followed by `slot.request(..)`, is not
traced: only method receivers and `let`/assignment targets are tainted.
Generic callees are closed. A generic parameter can call `request`/`fail`
only through a trait implemented for `Cx`, and such an impl is refused.
Review must reject the free-function store shape as deliberate evasion.

## Escape fallback (ruling claude-ddi2il, definition pinned with claude-4yjlw0)

Chasing each syntax shape is replaced by one conservative rule. A Cx-typed
parameter, a Cx-typed closure parameter, or an untyped or `_`-containing
closure parameter is possibly-Cx. Such a binding **escapes** on any use other
than these two:
- a member receiver (`cx.store`, `cx.request(..)`);
- an **analysed call argument**. That requires all of:
  - a lowercase callee, so not a wrapper or constructor (`Some(cx)`, `W(cx)`);
  - no `&mut local` co-argument, so `std::mem::replace(&mut slot, cx)` and
    `Vec::push(&mut v, cx)` escape;
  - not a method on a local receiver. `self` and concrete non-Cx typed
    parameters are excepted, because they could hold a Cx only through a type
    naming `Cx`, which is refused. So `v.push(cx)`, `h.set(cx)` and
    `v.push(W(cx))` escape;
  - a call that is a whole statement, block tail or returned value. A bound,
    assigned (to a name, field, index or deref target), chained, nested or
    macro-wrapped result escapes. That covers `let c = id(cx)` through a venue
    generic.

A closure that captures a possibly-Cx binding from outside it escapes, since
it may be stored or returned.

Where a binding escapes, its whole scope (the function or closure body)
counts every legacy reference and every method-named macro, exactly as the old
lexical scan did. Precision drops apply only where no Cx escapes. Outside
escaping scopes, a call counts on a possibly-Cx receiver. So do `Cx::m(..)`,
`<Cx as T>::m(..)`, or any path call handed a possibly-Cx argument. Type-name
refusals (aliases, fields, `Option<&mut Cx>` parameters, Cx-returning
functions, trait impls) stay.

REDs: each of 4yjlw0's holes plus a generic-identity escape, in
`test_an_escaping_cx_counts_every_reference_in_its_function` and
`test_a_generic_escape_counts`. Each escape shape makes two unrelated
references count, and the analysed shapes record nothing.

Rebaseline at `49f7d1857b6` (712 entries): 556 kept in prod and 1 moved to
test, both byte-identical, with 155 drops:

| reason | count |
| --- | --- |
| macro | 108 |
| self-helper | 29 |
| field read | 13 |
| other member | 4 |
| qualified | 1 |

The mapping JSON lists each drop. The self-helper drops are intentional: they
occur only where the caller's Cx does not escape, and the helper body stays
inventoried.
