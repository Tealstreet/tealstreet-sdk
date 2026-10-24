# Backpack supported-history Read custody

Owner codex-mxb4mq; native review codex-hcmq3f; credential review claude-m26ps4.

## Base and scope

The overseer explicitly authorizes building on #2090 at
`6e26539d0cfb726628c9534af567946198137b7d` (2026-10-25). This replaces the
previous local plan's requirement to wait for that seam's actual landing before
originals. The actual #2053, #2065, #2073 and #2078 landings are ancestors.
#2090 carries the exact native parity file from c5a8ebbd4bf; do not add it again.
Source qualification must integrate actual prerequisite landings before READY.

Remove one request fingerprint, history.rs
`0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` (count 1).
Four native routes (fills, orders, funding and public trades) keep one typed
history owner. Expected production change: history.rs and narrow lib callbacks,
roughly 150–250 lines, plus the consumer-owned Backpack publicRead declaration.
The provider seam itself remains m26-owned. Keep account/trading, shared public
get and its startup/seed callers, all other inventory and selection sites exact.

Open-PR file map checked before originals: #2090 owns registry provider; #2069
owns Backpack lib/trading dispatch seams; #2033/#1990 own dispatch/support
fixtures. No listed PR owns history.rs or the new history_context test. Keep
existing support files unchanged. Exact lib callback hunks need owner review
before application; no authority to edit group/engine/provider implementation.

## Reviewed API and native invariants

The prior fd8cec/0acacab proposal is direction-CLEAR by hcmq3f and m26ps4.
Keep #2053's initial validation/refusal precedence, then consume genuine
None/Expired/Remaining command receipt before selecting or allocating a root.
Private history uses actual #2073 Slots(identity, read_identity, default),
Read { scope: None }, and SelectedRead::command_root once. All page legs retain
that same selected binding. Missing/not-installed refusals match #2073 and
retain native message/kind/outcome; never synthesize a KeyRef or signed keyless
root. Public trades use admit_caller_read_root(None, None) and keyless legs.
Bind either genuine root through ReadCompletion::from_context, then one
ReadCursorHistory with the native CursorWalk(50), offset/stall/row encoder.

Each physical leg has a NEW 20,000ms maximum, clipped to the original caller
receipt; no root renewal, pacing or retry (Never). Declare this policy in
CALLERS. Private ReadRoot::leg queues the native sign::Http recipe through
real Prepare/Bind/Claim. Public http_leg_context queues the existing unsigned
static X-Broker-Id:1200 header and native Public egress. ArrayAtRoot streaming
is unchanged. Exact root/leg DATA and history admission precede row staging,
diagnostics/status and final success. Retire the current leg after admission
and before minting another; never retire the root at a page boundary.

Failure-only cleanup and accepted-prefix recovery use exact current Pending,
live original root and recorded failure custody; a released Timeout does not
need new DATA permission. Discard unaccepted rows, IDs and skip state. Keep
previous accepted rows and native incompleteness, strict root expiry, exactly
one terminal, and at most one live physical leg. No numeric fallback.

Preserve all URL/query/header/body bytes, GET secondary selection and primary
window, native row economics/high-precision IDs, fills [from,to), other inclusive
bounds, clamped limits, raw-row hashes, offsets, filtered/skipped neighbors,
50-page cap and native diagnostics. Public-only saved read keys must keep all
three private routes working: activate publicRead atomically with adoption,
without primary authority, private startup or hosted admission.

## Originals and qualification

First controls pin genuine 37ms timeout/DATA equality and beyond-expiry refusal
for all four routes, a working before-expiry empty response for each, second-page
remaining budget and original expiry, exact native primary/read-key wire bytes,
accepted-prefix failure recovery, and native public trade economics. Label the
20s physical cap as new policy rather than a native regression. Tests use real
provider identities and prepared_actions (real Prepare/Bind/Claim); do not fake
managed callbacks. Before production send immutable original/control receipt to
hcmq3f and m26ps4. Missing imports, compile failures and assertions after an
already-failed assertion are not RED proof.

Source-stage controls additionally cover delayed provider preparation, exact
leg retirement/zero live legs after 50 pages, page-2 failure accepted-only rows,
late streamed rows/diagnostics, stale/foreign DATA, duplicate/capacity refusal,
selection retention, and public-only provider evidence. Existing history tests
and inherited native registry parity must stay green. JS/capsule history parity
is a separate required gate, not inherited from native registry controls.
Scoped Cargo only, CARGO_BUILD_JOBS=4, one local build; strict checks and all
request/credential/signing/socket ratchets. Original controls are committed
before production. Push only a frozen draft after source review gates; one PR
Rust run, no duplicate dispatch. READY requires source CLEAR and qualifying CI.

## Original execution checkpoint

On the unchanged #2090 source, scoped history_context executes 21 tests:
6 native working controls PASS, 14 caller-custody assertion REDs, plus 1
explicitly NEW per-physical-page-cap assertion RED, 0 ignored. Four routes
currently emit no physical timeout and accept empty successful DATA at/even
after the genuine 37ms receipt; the page-2 controls demonstrate renewal risk.
No missing methods/imports or compiler failure are counted. Working controls
cover all four before-expiry completions, primary native wire/headers/window,
public-only saved-secondary three-route signatures, page-2 before-expiry
completion, public high-precision trade economics and accepted-page-only
recovery after a later streamed transport failure.

This is original-source evidence, not implementation or source-stage proof.
Native registry parity remains inherited unmodified; exact managed lifecycle,
retirement, hostile DATA and JS/capsule evidence controls remain gates. Receipt:
/tmp/codex-backpack-history-context-originals-receipt.json.

## Credential review follow-ups

M26ps4 direction-CLEARed 50071fa1648/58a651987f1 without independent execution.
Add the plan index row. Keep the native raw-helper positive unchanged; it is
not proof of provider signing. Three separate source-stage controls use a
real Secondary provider with seed [9;32] and raw read credentials with different
key/seed [8;32], plus the registry's read_credentials_status: Some(Ok(())).
Each independently pins the fills/orders/funding headers to the selected
provider identity and demonstrates that the raw identity's signature differs.
Only the final exact-header assertion is RED evidence; the later response
assertion remains gated until GREEN. No primary, fake ref or keyless signed
root is introduced. These are managed-adoption source gates, not changes to
the preserved native public-only behavior.

Hcmq3f bounded plan/native/original-source CLEAR at58a651987f1; no runtime
clearance or independent execution inferred. Expiry suffixes additionally
forbid managed Prepare/Bind as well as raw HTTP. The unchanged earlier failed
success assertion still gates those suffixes on original source. Source patch
must retain exact selected/private or keyless/public roots, DATA before any
item/status/diagnostic, released-failure/live-root recovery and page-local
row/ID/skip discard. One live leg, 50-page retirement, pressure and JS parity
remain explicit source-stage checks.

## Native primary-readiness source finding

Hcmq3f found that the first unapplied source proposal would select the installed
primary identity before native account_start had created account.credentials.
Native primary-only history refuses both before start and while the catalogue
is held. Preserve that behavior without a new refusal or support expansion.
Slots must receive the installed primary identity only when the actual typed
Option<Credentials> native state is Some; a real Secondary remains eligible
independently, including before start and in public-only mode. No key/secret
re-probe or separate credential-selection gate is needed. M26 reviews this
exact native-readiness filter before application.

Six working native controls pin pre-start/discovery-held refusals across the
three private routes (native message, absent kind/outcome, no physical work or
Store mutation). A separate Secondary positive covers both startup states and
private/public-only modes, retaining the native60000 window before primary
initialization even when recv_window5000 is configured. These are working
parity controls, not original-source failures or a behavior ruling. The old
a1dde121 source patch is NOT approved for application.

## Scoped qualification corrections

The source/native reviewers CLEARed compatibility patch 5608b94a on37a96.
Select once before allocating the selected root: a missing selection uses the
existing cause-aware local Read refusal, preserving native pre-start and held
catalogue actions. A hit passes its exact binding into command_root unchanged.
History alone presents a released Network failure with native Internal kind;
message, outcome and code remain intact. Preparation and Timeout retain their
typed causes. This preserves existing native assertions rather than changing
their expected results.

The released-page Timeout control delivers a real HTTP event at the released
deadline while the original caller root remains live; the engine's HTTP event
admission performs expiry. It does not synthesize a timer or failure receipt.
A separate test observes actual retained root/leg handles across fifty pages,
checks each retired leg is Revoked before the next release, and checks the
original root is Revoked at the native page cap. Filtered native rows preserve
empty accepted history plus its incompleteness notice at that cap.

## Source-stage execution checkpoint

After the reviewed compatibility correction, the scoped Backpack suite passes
274 tests, with the one pre-existing ignored account/background signer adoption
control unchanged. The 31 original/caller controls and 11 managed lifecycle
controls all execute without skips. Lifecycle includes full-context pressure
at page turnover, initial capacity refusal with genuine prior-owner delivery,
stale retired-page DATA, physical Timeout under a live caller root, and
page-two preparation/Network/status/codec accepted-prefix recovery. Strict
crate Clippy and formatting pass with the pinned Rust toolchain, jobs4.

The separate fifty-page observer checks actual exact handles through ordinary
venue callbacks, without a production counter or engine change. Its filtered
rows retain the native empty accepted result and one incompleteness notice at
the page cap. Every recorded physical leg and the final original root are
Revoked. The test's outbound HttpRequest is named packet so assertion macros
do not create lexical request-site false positives; it introduces no opaque
request/completion call and no inventory allowance.

The real Backpack WASM family builds. Two JavaScript integration controls run
the actual EngineHost and native Prepare/Bind/Claim/dispatch pipeline. Across
fills/orders/funding, public-only saved read keys sign exactly the independent
Node Ed25519 oracle with the native60000 window; malformed primary material
is never probed. No raw HTTP, hosted signer or socket is used, missing keys
retain native refusal metadata, and transport resources/timers settle. This
is separate JS/WASM evidence, not inherited native registry proof. The local
build has no wasm-opt, so optimized size remains a CI gate.

Remaining freeze gates: carry the undeclared-policy registry working control
through the owner-reviewed Install test override, integrate the actual tip and
prerequisite landings, qualify inventories on that composition, obtain exact
committed preservation reviews, then take one PR-triggered Rust run. No READY
or landing clearance is claimed by this checkpoint.

## Actual provider landing and policy controls

The provider owner CLEARed test carry552eda30: the undeclared registry control
now explicitly chooses Install through its existing test-only override, with
all assertions intact. Separate pins require the actual Backpack declaration
and both registry/JS bridge records to be PublicRead. The real JS signer setup
capsule drops primary material in public-only mode and retains the secondary;
the ten scoped read-key policy controls pass. Other families remain unchanged.

The fifty-page private observer fixture lives under tests/support and is
compiled only by the cfg(test) unit module. Production source inventories
conservatively include src test fixtures, so putting deterministic test signer
setup in the test-support directory avoids adding four test-only material
sites to the production signing baseline. No production site or assertion is
hidden or removed, and no inventory allowance is added.

The actual provider seam is now landed as8efb4548c39755a6a180b5a381de7c58fafb06c3.
Integrate that immutable commit, rerun originals against its native source, and
carry compatibility/source reviews before qualifying the composed candidate.
