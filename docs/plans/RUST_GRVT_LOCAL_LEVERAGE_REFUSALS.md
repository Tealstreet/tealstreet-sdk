# GRVT initial leverage Read refusals

Plan-first base: actual5d825c7b96548c533493d8143f37ef1da32a0d27.
The overseer granted exactly two local account.rs terminals, preserving native
market-before-account precedence, messages, kinds and outcomes. This checkpoint
contains the plan only. Original controls precede any source change.

## Scope and inventory

`Grvt::on_command(FetchLeverage)` calls fetch_leverage_command directly in the
genuine command callback. Its first two branches are synchronous local Read
refusals, before serializing or issuing the private leverage request:

| Branch, in native order | Exact native text | Kind / outcome |
| --- | --- | --- |
| Store has no requested market | Unknown GRVT leverage market | None / None |
| Market exists, trading_account_id absent | GRVT private account is not authenticated | None / None |

Neither branch selects a key, signer, Session, route or account subject. Preserve
that ordering with no credentials and with configured credentials that have not
established an account. Public-data-only does not bypass either native refusal.
The symbol lookup still precedes the account check, so an unknown symbol must
never become an authentication diagnostic.

The current account inventory has11 occurrences. Remove exactly two singleton
fingerprints, leaving9:

- 5dcc3ef2d5d2fc61a53a8e99f5df87d9a5678caece10fe1966cb422429db3b54
- 1a500310d6ed84a8168e03b0b0e5ad5c7c62303e2788fa0bdbc19c57d193e16e

GRVT has34 occurrences on this actual base; this slice alone gives32. The
separate #2076 fixed-setting slice removes3, making the combined projection
31->29 after it lands. Do not claim the pending removal or touch its trading key.
Only the account manifest key may shrink; foreign raw bytes/order stay exact.

## Shared entry transaction

Only inside each native refusal branch, admit_http_command with Read, no
credential,20s, no pacing and Never establishes genuine command authority. A
successful admission immediately calls fail_context with the exact native string.
No journal or financial outcome is introduced. Native kind/outcome remain None.
A shared admission rejection owns its terminal and returns without a numeric
fallback. There is no HTTP leg, Plan, retained root, retry or renewal.

Shared capacity, clock, duplicate and caller-receipt checks now precede the
native local diagnostic. CALLERS must disclose this newly enforced ordering.
At foreground-only context/command capacity the shared failure is Internal with
Read outcome None; invalid-clock admission is InvalidInput with outcome None.
Neither case may emit a second failure or command-error diagnostic.

The valid-account fallthrough, its exact body and original private request are
byte-exact. Account/login Sessions, cookies, authenticated subjects, credential
selection, signers, hosted construction, deferred waiting, leverage revisions,
background polls, queues and all callbacks are excluded. No tealstreet-next work
is performed in this mirror. No missing engine API or bootstrap is expected.

## Compiling original controls

Use a thin Venue delegator forwarding real Grvt::on_command. Startup may seed a
single observable Store market, without login, credentials or synthetic tested
terminals. Peer commands admit and retain genuine Command Read roots through
real Engine entry. No fabricated Context or numeric completion substitutes.

Separate controls pin both native errors and full metadata, with no credentials,
public-only options, and configured credentials without a native account. An
unknown symbol with no account must keep the market diagnostic. Check complete
action batches for raw/managed HTTP, provider signing and socket activity, and
for a duplicate command-error notice. Store markets/settings stay unchanged.

For each refusal, fill256 genuine Command Read owners, then assert one shared
Internal/Read-None refusal instead of the native untyped failure. Retire the
exact first peer, repeat the same command and assert the original native error;
validate all255 retained peers. Those recovery suffixes are gated by the first
RED on original production and must not be represented as executed proof.

For each branch use genuine Event::Command with NaN to expose the shared clock
boundary, not handle_command's checked pre-rejection. Also cover zero caller
wait and duplicate commands retaining a real original owner. Original REDs must
compile and fail their intended assertions; fixture/setup or missing-method
failures are not evidence. Existing native account controls remain unchanged.

## Review and execution gates

Commit this plan before original controls. Native/preservation review is9czdjh;
core/API original review isfe7lgm. No source change until those originals are
CLEAR. Preserve every original assertion through adoption. Source scope is only
the two account refusal branches plus required imports, own inventory, dedicated
test, plan and standard publication docs. No lib/trading/identity/history edit.

Use one scoped Cargo process at a time with CARGO_BUILD_JOBS=4. Clean only the
own target between builds if above15G; no ENOSPC-window check is evidence. Strict
changed-crate Clippy/fmt and ratchets follow production. Workspace/all-feature,
WASM and performance checks remain in CI. Push only an intended freeze draft;
one PR-triggered Rust run per head, no duplicate explicit dispatch. Source CLEAR
and green exact-head CI are required for READY.

## Original execution checkpoint

The unchanged actual5d825 source compiles and executes12 controls:4 intended
assertion REDs/8 working GREENs/0 ignored. Both capacity cases fail at native
kind None instead of shared Internal, and both NaN Event cases fail at native
kind None instead of shared InvalidInput. No fixture/setup, missing method or
compile failure is counted. Native controls independently confirm exact strings,
kind/outcome/code/hint None, one terminal and no I/O or Store change.

Working controls cover market-before-account precedence, known market without
an account, public-only and configured-but-unestablished credentials, both
zero-wait entries and both duplicate commands retaining their original owner.
The exact first-peer retirement, native recovery and all255-survivor validation
remain gated suffixes on original source. Production, existing fixtures, Cargo
and every inventory stay byte-exact the actual base. No plan/RED checkpoint is
pushed for CI.

## Production checkpoint

After core and native original CLEAR, the two refusal branches admit keyless
Read20s/Never and fail_context with the exact native string. All12 original
controls pass, including the previously gated recovery/255-survivor suffixes;
all44 existing account controls pass unchanged. No test expectation changed.
Only account imports/two local exits and the exact two account inventory rows
change in production. CALLERS discloses newly enforced admission precedence.
Immutable source review and intended-head CI remain required before READY.
