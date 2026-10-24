# Breakout immediate local command custody

Plan first, 2026-10-24. Owner codex-hcmq3f. Fresh branch
`feat/rust-breakout-local-command-context` starts at actual
`db6fa45bbb7cbbfdfeb0aa40d3dd93caac9c9e0b`. The overseer approved seven
occurrences, reducing the canonical request inventory from21 to14. Production
waits for original assertion REDs and independent review.

## Exact local scope

| Source at the base | Occurrences | Selected behavior |
| --- | ---: | --- |
| lib.rs:1187/1191 | 2 | Unsupported candle interval / invalid ISO bounds before schedule. |
| lib.rs:1218/1219 | 2 | Local null leverage / unavailable maximum leverage. |
| lib.rs:1228 | 1 | Mixed unsupported command arm, reached genuinely by book and STP commands. |
| trading.rs:531 | 1 | Fixed leverage/margin/mode refusal before any job. |
| trading.rs:549 | 1 | Pure initial trade-planning error before job insertion/advance. |

on_command is the only caller of on_trade_command. No timer, response, retry,
queue drain or session callback replays the selected command handlers. Valid
candle requests still schedule legacy work; valid trade plans still enter the
native queue. No physical request, response, login, account, generation, retry,
pacing, signing, accepted-ID or financial reducer change is selected.

Keep the native option-refusal occurrence held: engine capability checks reject
those commands before venue entry. The duplicate-job occurrence also stays held:
the ordinary guarded write-entry path intercepts live duplicate IDs before the
native job guard. Do not invent a wrapper/capability, fake Context or numerical
entry to claim either as an original assertion RED. The mixed unsupported arm
also contains FetchAccount and symbol-aware amendment; preserve the existing
engine pre-venue refusals for those commands in passing controls.

## Shared ownership and caller contract

Use existing genuine initial entry, with Read for local queries/validation and
Mutation for write refusals. The mixed unsupported arm must derive Operation
from the actual command (SetStpMode and amendment are Mutation), never from its
message or credential presence. Trade setting refusals likewise follow actual
operation; no successful Mutation is selected.

The proposed local projection checks command_wait first. None uses
reject_unstamped_command, Expired uses reject_expired_command, and Remaining
admits the existing Policy with credential None, no pacing, retry Never and an
explicit 20,000ms bound, then immediately done_context or fail_context. No
physical I/O or timer is staged. Rejected already emitted its one terminal;
Revoked grants no completion or numeric fallback. Preserve the native message,
JSON, kind and validation order.

Caller-visible change to pin explicitly: the legacy mixed unsupported
SetStpMode branch has an untyped error without an outcome. Genuine keyless
Mutation ownership truthfully projects NotSent for that pre-I/O refusal.
Document this added field in CALLERS; do not claim byte-identical failure
metadata for that case. Its native message and absent kind remain intact. Read
refusals retain no money outcome. Fixed-account-setting/build refusals retain
native NotSent and their existing kind. This is not LocalSetting success.

## Train and selection boundaries

The actual be710..train/combo-5b-be71 runtime map contains no Breakout source.
Selected production files would be Breakout lib.rs, trading.rs and a local
helper only, plus focused tests and own docs/request manifest. No engine, host,
registry, construction rules, sign/session/account/history implementation or
Cargo feature change is needed. Check the actual train map again before source.

Until5b lands, add/remove NO key-selection or nonce site and do not edit the
credential-selection manifest/checker. Preserve login credential reads,
public_only/account routing, raw session custody, key-choice and entropy source
sites exactly. The planned helper declares no credential; it chooses none of the
configured keys. Existing UUID/client-ID construction stays native. The new
phase tests are outside venue src and do not rewrite production to expose hooks.

## Original controls and delivery gates

Commit this plan, then separate controls, then execute on unchanged production.
Invalid-clock Engine commands must refuse with shared InvalidInput instead of
native completion. Cover both candle branches, local leverage and maximum
leverage, genuine mixed Read/Mutation commands, all three fixed setting commands
and a real trade-planning validation failure. Separately label the finite
SetStpMode NotSent projection as NEW-CONTRACT. Keep native finite-clock JSON,
messages, kind, settings/financial Store, no-I/O and settled-ID reuse controls.
Pre-venue capability and duplicate protection are working controls, not invented
regressions. Never count compilation or inaccessible APIs as original REDs.

After review, all original assertions stay unchanged through production. Prune
exactly lib5 + trading2 request occurrences, preserving all other entries and
all selection counts. Run only scoped changed crate/test Cargo checks with
CARGO_BUILD_JOBS=4, one local build at a time, plus fmt/bans/ratchets. Independent
source/caller/map review and fresh Rust/family WASM size/performance remain gates
before READY. No CI/merge or whole-family/session custody claim from this plan.


## Fixture corrections before the original receipt

The first compile used a nonexistent public Store::settings getter; this is
excluded from RED evidence. Existing setting/market getters replace it without
adding any engine API. The first native range fixture used -1ms, which DXtrade
accepts as an ISO timestamp: it reached legacy login instead of the intended
local validation. The corrected case uses an infinite bound to exercise native
iso_time refusal. A separate passing control requires the real physical login
path for -1ms, pinning that no negative-time restriction may be introduced.
No production or inventory change accompanies either fixture correction.
