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
