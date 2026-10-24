# WEEX leverage initial managed admission

Status: plan and original assertion evidence; production held.
Actual base: `9a757d406b8`.
Required parent: #2055 `911d20cc06a` (local leverage refusals).
Composition: plain merge `24494f352e8`, with both CALLERS append sections kept.

## One-site scope

The remaining numeric failure in lib.rs fetch_leverage runs when managed::read
returns Err. It has fingerprint
`c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`, one
occurrence in this slice; other equal fingerprints are outside it. managed.rs::read
has exactly this one caller. No physical request site is pruned by this change.

Replace only that numeric fallback with the existing initial command Read
transaction. Prospective runtime files: WEEX managed.rs and lib.rs. Keep
SelectedRead::select, its role/scope argument, stored Slots, credential policy,
identity snapshot, dispatch recipe, route and egress bytes exactly unchanged.
No key-selection site or credential inventory change is allowed before 5b.
No account, trading, shared engine, checker or STAGING edits.

Combo-5b names only BingX account.rs among BingX/WEEX source at this actual base.
The overseer ruled that this slice stacks after #2055 because both edit lib.rs.
#2003 and #2042 also touch lib.rs; integrate their actual landings in train order.
#1747 remains separate: the native settings revision and managed response
publication stay unchanged, without new metadata ownership or write fences.

## Existing behavior and gap

The market, paused-read and missing-selection refusals are handled by #2055.
Once those pass, the caller retains the native settings revision and sends one
GET `/capi/v3/account/symbolConfig?symbol=<native market id>`.

managed::read currently uses SelectedRead::scoped under Command(command). That
constructor returns an admission error without settling it. The helper discards
the cause, returns `WEEX managed read lifecycle is unavailable` with
Internal/NotSent, and the caller emits it numerically. Full registry capacity or
invalid receipt clocks can therefore lose the shared entry-refusal cause.

The successful path remains one signed managed physical request: 20,000 ms,
Never, no pacing, no retry, same selected identity/passphrase, native headers and
query, same native response/error reducer and settings publication. The original
per-call bound is retained even when an outer caller wait is shorter. This slice
does not add an engine whole-query caller deadline or a parallel history root.

## Initial transaction and terminal ownership

Keep the existing Selection producer byte-exact. Inside the single-caller helper,
use the cause-aware command_wait guard: None uses reject_unstamped_command(Read),
Expired uses reject_expired_command(Read), and Remaining proceeds. No new receipt
default, guessed command id, numeric fallback or borrowed callback is permitted.

Then call existing SelectedRead::command with the same selection snapshot and
native ReadPolicy (20,000 ms, Never, no pacing). Its genuine Fresh-entry
transaction emits its own canonical Read failure on denial. Return immediately
without an additional venue terminal. The defensive LocalRefusal keeps the
existing lifecycle error; normal callers already refuse missing selection
before this helper. No new selector or key-availability rule is introduced.

After admission, retain the existing Context, request recipe, queue closure and
queue-refusal fail_context path. Provider/preparation and queue failures retain
their native typed fields. The complete/reject helpers and all callbacks stay
unchanged. Remove the obsolete Result/command-id handoff and caller Err arm;
there is no dual completion path or compatibility numeric fallback.

The receipt guard checks initial provenance only; it does not renew or replace
any retained receipt or reinterpret the native physical bound. Shared expired
or borrowed-entry behavior remains source/API evidence unless genuinely driven.

## Caller-visible contract

Initial capacity now returns the canonical shared Read Capacity cause with no
write outcome, instead of the opaque lifecycle error with NotSent. An unstampable
clock returns Revoked / `Request context was revoked`, InvalidInput and no
outcome. Preserve the original live managed success, native provider/queue error
metadata, no auth evidence, settings values and ID reuse. Document these exact
entry-refusal deltas in RUST_CUTOVER_CALLERS.md with production.

## Compiling original evidence

Add only tests/leverage_admission_context.rs, this plan and one index row. The
stacked parent runtime is unchanged by this checkpoint; compare against
`24494f352e8`, not the unstacked base when counting own paths.

Expected separate REDs use the actual WEEX engine and native FetchLeverage:

- Occupy all 256 owners using genuine managed FetchAccount identity requests and
  real Prepare/Bind/Claim. A real overflowing FetchAccount gives the full
  canonical Read Capacity error. Compare the leverage fallback's full error.
- At NaN, valid credentials and market reach managed admission; compare its full
  error to shared Revoked/InvalidInput/outcome None.
- A nonadvancing f64::MAX clock likewise has no receipt and must fail closed.

Every case is independent, with no missing-method, fake error adapter or
private-state mutation. Capacity setup pins actual native identity URLs and
request count. Settings and unrelated owner/recovery suffixes remain explicit
post-cutover gates when a RED stops first.

GREEN controls pin successful signed leverage output and exact signature oracle,
primary/secondary-only policy, the native 20 s physical bound with both default
and short configured host wait, provider refusal metadata/no auth evidence,
real overflow/peer recovery, and duplicate ownership. Startup is not run; do not
claim a complete public/private handshake. Keys are fixture placeholders.

Run only this target, CARGO_BUILD_JOBS=4, serially, then scoped strict Clippy/fmt.
No broad local workspace/all-feature build. Production and inventory updates are
held for bounded native/core review and explicit production authorization.

## Original assertion receipt

Plan commit `46da29d787c` preceded the new target. Executed on stack `24494f352e8`
with the existing #2055 parent source unchanged:

```sh
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2 cargo test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-weex --test leverage_admission_context
CARGO_BUILD_JOBS=4 cargo clippy --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-weex --test leverage_admission_context -- -D warnings
```

Result: three independent assertion REDs, seven GREEN controls, zero ignored.
No compilation, setup, missing-method or missing-request failure. All REDs
compare full VenueErrors. Capacity produces lifecycle Internal/NotSent instead
of the actual overflow's canonical Read Capacity/outcome None. NaN and f64::MAX
produce that same lifecycle error instead of Read Revoked/InvalidInput/None.

The capacity RED first releases 256 real native accountConfig identity requests
through Prepare/Bind/Claim; no fixture-only Context or substitute venue is used.
The separate capacity GREEN completes the original native identity owner,
issues a fresh leverage read in the freed slot and completes it successfully.
The RED's subsequent settings and recovery suffix, and each invalid-clock RED's
finite re-ask suffix, remain explicit post-cutover gates because their first
error assertion stops today.

Other GREENs pin primary, secondary and secondary-only full headers against the
independent signature oracle and native output/settings; a configured 1234 ms
host wait still has the native 20000 ms physical bound. Provider refusal keeps
its exact typed NotSent error with no HTTP/auth/deactivation or Store effect.
Duplicate command ownership preserves one original success and fresh ID reuse.
No Start/private handshake, deferred-entry or queue-capacity fault injection is
claimed. Scoped strict Clippy and rustfmt pass; CI/broad local builds were not
run for this checkpoint. Its only own paths are this plan, README and the new
test, compared with `24494f352e8`; runtime and inventories remain parent-exact.
