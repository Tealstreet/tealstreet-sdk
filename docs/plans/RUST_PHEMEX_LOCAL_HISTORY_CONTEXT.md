# Phemex local history command Contexts

Lane codex-ih35js, 2026-10-24. Base actual landing
661427d10b3747927198e52ca239c4a5d4601826. Plan precedes original assertion REDs,
which precede implementation. After #1869 lands, integrate actual 1e8f600da75 and request
a train window after clean reviews and CI; no merge without explicit GO.

## Selection and scope

This phase migrates eleven synchronous local history/income terminals to genuine
command-entry admission and Context completion. It is the largest available exact
shrink in the five-venue group: eleven occurrences/eleven entries, versus four/two
for Crypto.com's complete public candle request. Public WOO candles could remove
six/five but require unsigned Primary command-entry egress; the shared owner
confirms no reviewed pin. Aster source is held by #1893's signer owner. Private
HMAC paths await the shared read helper after #1817, and managed pageable history
still needs the shared Context-bearing Completion bridge. None is replaced here.

Both the 48 open landing-base PRs and all 66 open PRs across bases were inspected.
No PR claims Phemex history.rs, income.rs or the new test. #1788's WOO/Crypto.com
account/trading/test claims remain separate. HMAC owner claude-4yjlw0 confirms
private migrations have not started and still wait for the shared helper.

This is a local-terminal conversion, not managed HTTP/history adoption. Successful
native history/income commands retain their original signing, pending records,
shared HistoryRows/RangeHistory/Completion, pagination, pacing and retries. No
Context is issued on those valid native paths, so there is no post-issuance
numeric fallback. No public request is relabeled to mask private transport.

## Engine path and caller behavior

A narrow local-result adapter in history.rs takes no command ID. Only an already
selected synchronous terminal branch calls it. It borrows current engine custody
through admit_http_command(Read, no credential, 20-second bound, Never), then
calls done_context or fail_context. Rejected already publishes the engine's one
terminal; Revoked grants none. Both return without another terminal or dispatch.
This uses actual #1830 initial Read custody and Context finalizers, as actual
Lighter #1900 does for cached reads. No HTTP is submitted and no timer is needed
for synchronous completion. Credential None is used only where no credential,
signing, route or account-auth operation occurs.

Finite admitted calls preserve exact legacy payloads, failure text, kind=None and
outcome=None. Unknown client-ID lookup remains
{"found":false,"definitive":false}. It does not claim authoritative history
absence or financial coverage. Invalid engine time or exhausted owner capacity
now produces the engine's typed rejection before a local result. Repeated or
revoked entry cannot manufacture another terminal. No account-auth, Store or
financial publication is introduced.

Native refusal precedence stays unchanged: invalid client ID, unknown-symbol
lookup shortcut, credentials, known symbol, bounds, active limit/duplicate, then
native-symbol safety. Income keeps credentials, symbol filter, bounds/limit,
then active limit/duplicate. Check active len/contains before native-symbol
validation, but insert only after all local rejection branches have passed.
This avoids a local managed branch mutating active state before admission and
preserves the old precedence. Existing active owners must survive local refusal.

## Exact inventory

| Source | Before entries/occurrences | After entries/occurrences |
| --- | --- | --- |
| account.rs | 3/3 | 3/3 |
| history.rs | 13/16 | 6/9 |
| identity.rs | 3/3 | 3/3 |
| income.rs | 6/6 | 2/2 |
| lib.rs | 17/18 | 17/18 |
| trading.rs | 4/4 | 4/4 |
| Total | 46/50 | 35/39 |

The existing full-venue map in RUST_PHEMEX_REQUEST_CONTEXT.md covers deferred
signing, network and pager sites. This phase removes only these eleven exact
single-occurrence fingerprints:

### venues/phemex/src/history.rs

- `7f284f04d3b73fcbb59e861ae7f25e8aadc122a371d2e30d308dc9e01d28c312` ×1
- `05967623e28d0acd558da8192dba9a5d1f571a4e57337bd4a354d59c3a6bbdcc` ×1
- `5cc3422c65cca92ebe04677ec96d5f5a144bef89e045df63c4dd1150517c8191` ×1
- `6db5f410d6f00a616c4990ff32d4640e1a8db6f9d560ebe13c303d81dc823d15` ×1
- `639272e32bb3ce8f877fa8630b1f2feb1aa14970ef429c1ba5b17c19d9227cbf` ×1
- `1ca3b53997ab5d41fcef35db137f612b4836198fb1116731d22d513e4cb34e0f` ×1
- `e74f33a4a874839f7f8b471e8b36279ed94f190b2780d57e59f1cf14d23869a5` ×1

### venues/phemex/src/income.rs

- `5cc3422c65cca92ebe04677ec96d5f5a144bef89e045df63c4dd1150517c8191` ×1
- `238884203c8b673bc8d07d3f21f1c808fce23b0710a33064f25eb76aac6fec24` ×1
- `b23d2cd6eab1b45e491401cab80ff14bae48a9fa2a910e93f3400a669413b859` ×1
- `e86a99cf9fac6ceb0ab993b14a362b03d6711e49e03f0c6c8da4b2cbed7e94c9` ×1

History's seven cases are invalid client ID, nondefinitive unknown-market lookup,
missing credentials, unknown symbol, invalid bounds, active capacity/duplicate,
and unsafe native symbol. Income's four are missing credentials, forbidden symbol
filter, invalid bounds/limit, and active capacity/duplicate. Retain every other
fingerprint and every incoming unowned inventory entry; add no scanner exception.

## Original REDs and implementation order

1. Commit this plan and independent plan audit before code.
2. Commit a new request_context_local_history integration test separately, running
   it against unchanged 661 production. Capacity controls fill the actual
   256-owner registry with managed unsigned candle peers, then exercise a local
   failure, local lookup result and income failure. They must require the engine's
   typed refusal rather than the native local terminal. Invalid-clock controls
   reproduce the same ownership gap. Capture assertion failures and passing finite
   compatibility controls; no compiler-error or fake-token RED qualifies.
3. Add finite native compatibility for all eleven branches, both active-limit and
   duplicate subcases, exact nondefinitive lookup payload and error precedence.
   No I/O/timer, auth or Store publication may occur. Terminate one parked managed
   peer through the actual preparation-failure callback to free its registry slot
   and prove capacity recovery and isolation of the remaining owners. Preserve
   active native requests, then complete them through their original continuation.
4. Add the local adapter, replace only the selected terminals, and defer active
   insertion without changing valid native behavior. Keep valid public and signed
   request controls, payloads, original history assertions and paging tests.
5. Prune exactly the proven eleven entries, append CALLERS/AUDITS/QUALITY, run
   Phemex plus Binance native suites, scoped fmt/strict Clippy and both inventory
   checks. Push a PR into feat/rust-rewrite and watch exact-head CI.

Intended source changes: only history.rs and income.rs. Add
sdk-rs/venues/phemex/tests/request_context_local_history.rs. Shared bookkeeping:
request-legacy-sites.json, this indexed plan, README, CALLERS, AUDITS and QUALITY.
No lib.rs, account, trading, signer, registry, host, engine, codec, policy or
existing test edit is planned. If native fixture compatibility requires one,
audit its collision and assertion preservation before changing it.

Independent source/adversarial, caller and full-map reviews must be clean.
Architecture gate: No per-venue lifecycle implementation; the adapter consumes
the shared engine admission and terminal APIs. A fresh separate guard and
head-matched merge require an explicit post-freeze overseer window. No managed
network/history, complete private parity or full-package provenance claim is made.

## Original-source evidence and plan reviews

All three independent plan reviews are clean at 1f6f234597d: source/adversarial,
caller and full-map. Architecture gate: No. Capacity fixtures have neither
credential container constructed, so all 256 candle peers use actual managed
admission. Merely claiming a physical request would not free a registry slot;
the recovery control settles a peer through preparation failure.

On unchanged 661427d10b3 production, the new integration target compiles and runs
nine controls: five pass, four fail assertions. The capacity history/income
refusals still have kind=None instead of Internal; the unknown-market lookup
still succeeds despite full registry capacity; invalid-clock local history
still has kind=None instead of InvalidInput. No compile failure is counted.
The passing controls cover finite local payloads/precedence, native active
capacity and duplicate preservation, repeated local reuse, and valid signed
and public native dispatch/continuations. The engine itself rejects duplicate
active command IDs before venue entry with COMMAND_DUPLICATE and no terminal;
tests preserve that actual behavior and complete the original native owner.

## Implementation receipt

Original-RED commit: 0f089c48b59. Actual #1869 integration: f7c99052d4b
(parent 1e8f600da75), with only the README index conflict resolved incoming-first
plus the one Phemex row. All nine controls pass after the local adapter cutover;
no valid native path issues a Context. The exact scanner delta matches all eleven
planned hashes and changes no other source inventory.

Local verification on the actual-#1869 integration passes: Phemex and Binance
491 tests across 39 targets, zero failed/ignored; strict all-target Clippy and
scoped formatting; exact request inventory and its fifteen regression controls.
The original public/signed history and income tests are unchanged.

Independent implementation reviews confirm the exact prune, incoming prefix
preservation, unchanged native send/page/response code and shared-only admission
authority. Review corrections keep replacement-dispatch clocks monotonic and use
the actual SDK caller names in CALLERS; the focused controls and strict checks
are rerun after those corrections.
