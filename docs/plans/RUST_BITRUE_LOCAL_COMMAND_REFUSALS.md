# Bitrue synchronous local trading refusals

Owner: codex-9czdjh. Baseline: actual `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Branch: `feat/rust-bitrue-local-command-refusals`. Keep #1983 and KuCoin #1986
on their pushed separate branches. This is the overseer-approved next three-site
slice after KuCoin's eight refusals; it introduces no shared engine mechanism.

## Exact inventory and claims

Bitrue has 33 occurrences / 20 fingerprints across four source files. This slice
converts three synchronous initial refusals in trading.rs (12 -> 9 occurrences,
8 -> 6 fingerprints), yielding family totals 30 occurrences / 18 fingerprints.

| Initial native branch | Fingerprint prefix | Exact decrement |
| --- | --- | --- |
| SetMarginType unsupported | 5e1ebc068cca | 1 -> 0 |
| SetPositionMode(true) unsupported | cb0a44751a15 | 1 -> 0 |
| initial trade_plan Err | 2f0770d2ea5f | 2 -> 1 |

The second trade_plan-related numeric failure belongs to the already queued
remote flow and stays legacy. Never rename it or prune both matching occurrences.
Other files and all other manifest mappings remain incoming byte for byte.

A fresh open-PR file map shows trading.rs and the proposed isolated test path
clear. claude-4yjlw0 confirmed no unpublished claim on these local refusal regions.
The later HMAC/private cutover stays in its #1718 queue. SetPositionMode(false)
remains held for uufub4's #1989 local-setting success seam: it updates hedged=false
and returns [] before credentials, including keyless/public-only configurations.
No account/group, signing, socket or private request helper is changed here.

## Native behavior and shared API

Only after the existing branch selects a local error, call genuine
admit_http_command with explicit Mutation, credential None, timeout 20,000ms,
no pacing and RetryPolicy::Never. This is terminal-only authority for a branch
that issues no remote work. Return on Rejected/Revoked without numeric fallback
or another terminal; on success fail_context with the original not_sent error.
A small local policy declaration may live in trading.rs; admission, generation,
capacity and terminal algorithms remain in the engine.

Preserve error text, kind and outcome: unsupported margin/hedged modes stay
Unsupported/NotSent; initial planning errors keep their native kind and NotSent.
The unsupported settings are refused before credentials. trade_plan checks
public_data_only and key/secret availability before command-specific validation;
retain that ordering and any builder identity entropy consumption. Keep the
existing acknowledged-ID housekeeping before branch selection unchanged.

No owner is admitted on valid queue/private fallthrough. The queued retry and
trade_next error site stays unchanged. Native successful one-way mode is not a
Read and cannot use an empty Mutation journal or fabricated credentials/facts.

## Plan, original REDs and gates

Commit this plan and review it independently before tests/source. An isolated
local_command_refusals integration test forwards genuine Venue guard/callback
behavior. In a prior no-ID callback, retain 256 real Background Read Contexts;
each of the three native refusals must then return the exact engine Capacity
error with NotSent instead of bypassing command admission. Commit those compiling
assertion REDs separately; missing APIs or test compilation errors are not REDs.

Working controls pin keyed/keyless/public-only native error ordering, no HTTP or
signing-provider work, unchanged Store, capacity release/recovery, duplicate
original ownership and command-ID reuse. Check valid keyed private wire under
registry saturation, proving no new owner is admitted on remote fallthrough.
Pin successful SetPositionMode(false) as an unchanged working control only.

Then implement the three sites, prune precisely the two singleton fingerprints
and decrement the shared count once. Run focused Bitrue tests and strict Clippy,
scoped fmt, Binance request-context fast lane, both request-context checks and
three independent source/adversarial, caller and full-map reviews. Record the
admission-visible change in CALLERS/AUDITS/QUALITY. Open a separate draft into
feat/rust-rewrite; CI, current integration and an assigned guarded train slot
remain merge gates.

Architecture gate: No per-venue cross-cutting implementation. This uses existing
shared genuine entry and Context failure; keyless successful settings and private
signing remain gated on their separately owned shared/adoption work.

Plan reviews: source/adversarial, caller and independent scanner/full-map are
CLEAR at af731bfc50e. trade_command returns bool: admission refusal and completed
local failure must both return true, so lib cannot redispatch a handled command.
