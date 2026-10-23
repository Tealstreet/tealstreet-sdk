# GRVT synchronous history Read terminals

Plan-first owner codex-zs5e83. Base actual `6a6845c4491` on branch
`feat/rust-grvt-local-history-context`. Read with RUST_REQUEST_CONTEXT.md and
RUST_GRVT_REQUEST_CONTEXT.md. No production changes in this checkpoint.

## Ownership and exact scope

Migrate only seven immediate local refusals in history.rs::start_query. The fresh
open-PR file list has no history.rs or new request_context_local_history.rs claim.
Session owner claude-55ao7t confirms no overlap with #1905, whose history-adjacent
change is only the existing tests/history.rs action drain. New fixtures use real
login through test_support::prepared_actions, preserving compatibility with its
managed login adoption. Keep account.rs, lib.rs, sign/trading/identity, all existing
tests, remote history, headers, waiting, paging and finalizers byte-exact incoming.
No signer/session/Plan/bridge prerequisite is needed for these local branches.

## Native order and authority

Keep every condition and native message in its current order:

1. Unsupported Income category: `Unsupported GRVT income category`, before auth.
2. Non-public query without authenticated session: `GRVT query requires an authenticated session`.
3. Requested market missing: `Unknown GRVT query market`, before bounds.
4. Invalid bounds: preserve the exact error returned by native bound/scaled_u64.
5. Reversed range or empty order ID: `Invalid GRVT query range or order ID`.
6. Lookup ID invalid decimal, zero or leading zero: `Invalid GRVT client order ID`.
7. Private query without subaccount: `GRVT query requires a subaccount`.

At each original branch, after its native decision and before publishing, admit
one genuine command Read via Cx::admit_http_command: credential None, 20000ms,
no pacing, RetryPolicy::Never. Rejected already emits exactly one typed entry
failure; Revoked cannot consume another owner. Both return without fallback.
After successful admission, fail_context on that exact original root with the
unchanged native error, handle the Result without unwrap or a second terminal.
Do not admit on remote fallthrough or change Request/Completion authority.
No helper, timer, local admission policy, queue, retry or new seam is introduced.

## Inventory

Actual GRVT history11F/11O becomes4F/4O; family53F/56O becomes46F/49O.
Remove only these seven singleton fingerprints, preserving all other bytes/maps:

- `d714610c15915805a059c4d8796cf1954a2908722189ac570a2c127a55963ee7`
- `625ef3cfd881af1a997b7e309af2712fabf1013668ca5ca403d4a59913d01d4d`
- `9d3eea4ddeb346d10f8ebebfbc4dbbcde5939983ebcb9e2530b7c7e718be6e17`
- `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb`
- `83b8e68cd8445af7475b790f03a56ef5d72018274c5416a79fcab14f7f58cd7d`
- `43e76c2e596d56215f4ecd2f1cd300980b527f24c83ec67fbc6a19d6e78c81b9`
- `0bc1914737ba4f26b1f4a43a7db513f0c98aafaa24bd482b6cb5b2e21ef9d704`

The four remaining fingerprints include remote issuance and conservative lexical
macro entries. They stay owned by their future caller conversion; no bootstrap,
rehash, scanner predicate or exception is added.

## Original assertion REDs and native controls

Commit a new dedicated test target before source edits. A delegating real Venue
retains genuine initial Read roots issued by Engine commands. With all256 roots
occupied, each local branch must return one Internal capacity failure with no
financial outcome or I/O/Store effects; retiring a peer permits the same refused
ID to return its native error. Invalid clocks (NaN and both infinities) must
produce one shared InvalidInput failure, then recover at finite time. These
assertions must fail on unchanged source; compile failures are not RED proof.

Working controls pin all seven native outcomes and precedence on live/testnet,
including unsupported income before auth, auth before market, market before
bounds, invalid bounds before range/ID, empty ID before lookup syntax and missing
subaccount last. Private cases drive the actual API-key login and native cookie
installation; never mutate private venue state. The login fixture drains through
prepared_actions. Retained-root duplicate ID denial must not consume its peer;
repeated immediate refusals beyond the owner bound must not leak roots.

Known-market public trades at full Context capacity must keep exact legacy POST
route, Content-Type header, body/query limits, native schema/completion and one
physical request. That fallthrough takes no unused Read root and is not claimed
as managed remote history. Existing native history tests remain untouched.

## Caller behavior and review gates

Finite admitted local errors retain exact native message/kind/outcome and native
precedence. Clock/capacity/custody refusal now owns the single publication; these
Read errors acquire no financial NotSent/Unknown outcome. No public signature,
query capability or successful remote behavior changes.

Independent source/adversarial, caller and full-map reviews before source and
again after implementation. Architecture gate: No; this consumes the existing
shared entry and original-root terminal APIs with no venue-specific policy.
Run original RED target, green target, GRVT and Binance native suites, strict
touched-crate Clippy/scoped fmt, bans and both request-context checks. CI owns
WASM/replay/size/timing. Open a separate draft PR with one >=120s watcher; request
a venue train slot only after green CI and clean reviews. Fresh separate
own-path/own-key preservation guard precedes any head-matched authorized merge.
