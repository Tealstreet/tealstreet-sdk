# Paradex private reads on the managed funnel (PR-3a)

Applies to Paradex SDK, browser/WASM, worker and native callers. Owner:
claude-m26ps4. This is the read slice of Paradex hardening PR-3. PR-3b (`/auth`,
writes) follows after its engine PRs.

## History queries

Fills, order history, income and the client-order lookup become command Reads on
the account's JWT Session. Each page is a leg of that read, with the current Bearer
attached by the engine.

- **One window per walk.** The whole page walk has one 20 s window from admission,
  and a later page does not renew it. Unanswered pages used to wait indefinitely.
  When the window ends on a sent page, the query ends like any lost page. With no
  accepted rows it fails as a read (no outcome). After earlier pages were accepted,
  it returns those rows marked INCOMPLETE. A walk of several pages must therefore
  finish within 20 s in total.
- **A page never sent stays strict.** A page held behind a JWT refresh that is
  refused, or that reaches the window unsent, fails the query NotSent. Accepted
  earlier pages are not published. A page that was sent and then lost publishes
  the accepted rows as INCOMPLETE, as before.
- **A refused `/auth` is typed.** It fails a waiting query AuthInvalid, NotSent.
  It used to be untyped (Internal).
- **Unlisted markets.** An unlisted market at admission still answers `[]`, or a
  non-definitive lookup miss. A market delisted between pages now fails the query
  NotSent; it used to answer `[]`.

## FetchLeverage

FetchLeverage leaves the write queue and becomes a command Read on the JWT Session,
with the same 20 s window and typed `/auth` refusal. The result shape and the store
update are unchanged.

## Inventory

Request-legacy rows for Paradex go from 19 to 14:
- history: the page read, plus the `format!` row, cleared by hoisting the page-size
  choice out of the macro (a lexical change);
- public: the `matches!` row, cleared by a lexical binding rename;
- trading: the two FetchLeverage terminals.

The write dispatch sites stay until PR-3b. NX needs no change.
