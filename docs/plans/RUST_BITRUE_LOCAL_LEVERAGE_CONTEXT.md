# Bitrue local leverage preflight authority

Plan before source/tests; base actual `65b7cc5eff04a7726e87ac8d922933049e2062ec`.
Scope only four synchronous pre-I/O refusals in account.rs::account_command for
FetchMaxLeverage and FetchLeverage: unknown symbol and absent private credentials.
HMAC owner claude-4yjlw0 and group owner claude-fe7lgm explicitly cleared this
source region. No leverage_queries joining, fan-out, account refresh, setting
revision, valid HTTP, send/sign/lib or socket code changes. Future group adoption
still owns the successful FetchLeverage path. Open-PR file lists must show no
selected account source/new integration-test collision before each edit.

## Exact inventory

Independent base has Bitrue42 occurrences/27 rows/five files. account.rs12->8
occurrences and10->8 rows; full venue becomes38 occurrences/25 rows/five files.
Only the two count-two rows for unknown market and absent credential are pruned.
History #1906's five-hash prune is independent: preserve/reapply both if it lands
first, with all other incoming entries retained. No whole-venue zero claim.

## Shared authority and caller contract

Each returning branch directly consumes genuine initial Read admission with a
pure policy declaration (credential None, 20,000ms, no pacing, Never retry), then
fail_context on that root and return true. Both admission errors return true
without numeric fallback; Rejected already emitted the sole typed failure.
None credential truthfully describes local work with no private remote data or
transport. Do not acquire an owner on any successful legacy HTTP/group path.

Preserve exact messages and precedence: resolve/unknown symbol before missing
credentials, even for keyless accounts. Both local errors gain InvalidInput and
NotSent metadata; the original companion-free timeline/no Store mutation remain.
Shared admission refusal keeps engine-owned kind/reason with no Read financial
outcome. No local registry, completion wrapper, group, fake HTTP, credential,
route, signer, retry or clock mechanism is introduced.

## Original-RED, compatibility and review gates

Commit tests first in disjoint tests/local_leverage_context.rs. Engine<Probe>
delegates both native guards and real callbacks, seeds the same market shape as
existing tests and calls genuine FetchLeverage/FetchMaxLeverage commands. Pin
unknown-symbol-before-auth and credential message/typed metadata on both commands;
use alias resolution for known markets. A separate no-ID callback allocates256
Background roots and asserts the real bound; repeated refusals preserve peers,
explicit retirement recovers at one clock. Invalid clocks and more-than256
fixed-clock terminal reuses must not produce a numeric fallback or opaque work.

Original assertion REDs pin typed metadata, shared capacity and invalid-clock
refusal. Compatibility GREENs pin text/precedence, no I/O/Store mutation and the
unchanged valid bracket/account HTTP paths. Do not assert new group behavior or
use a pre-existing pending query as admission authority. Valid requests retain
their native route, signatures and caller ownership, awaiting shared migration.

Require independent plan/source-adversarial, caller and full-map CLEAN reviews.
After unchanged-source REDs, freeze tests, adopt, require all GREEN, run Bitrue
plus Binance native tests, scoped strict Clippy/fmt, bans and both request-context
checks. Append CALLERS/AUDITS/QUALITY and README; prune only those two count-two
rows. CI owns WASM/replay/size/performance. Explicit train slot plus genuine latest
landing and separate fresh guard/head-matched merge remain mandatory.
Architecture gate: direct shared command authority only; no venue-specific
implementation of a cross-cutting concern. Group/signing prerequisites stay held.

## Exact selected fingerprints

- `17eccb5b6b6d70935f1f5ba5db37d97212ca0bfad15e0c30b719b58a6bbdab89` count2
- `b39921dbd50796a60a4e2f21febec858aef747570716300c27d2ec5c5512d0ad` count2

## Executed receipt

Plan `bbead8c09c7`, tests-first `74919ba4677`: original source gives four
assertion REDs and three compatibility GREENs; the same seven frozen controls
pass after adoption. All460 Bitrue/Binance native tests across38 targets pass,
including existing successful account/group completion controls. Strict scoped
Clippy/fmt, bans and both request-context checks (15 regressions) pass. Source,
caller and full-map independent reviews are CLEAN; CI/publication and explicit
train slot remain separate gates. Valid account/group source is unchanged.
