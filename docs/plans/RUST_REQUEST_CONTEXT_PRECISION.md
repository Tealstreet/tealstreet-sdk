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

The checker is still lexical. A `Cx` laundered through a generic std function
(`let c = std::convert::identity(cx)`) is not caught. Rules (2) close every
venue-defined route: no `Cx`-returning fn, no alias, no field, no trait impl.
Review must reject the std shape, as it would any deliberate evasion of the
old checker.

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
