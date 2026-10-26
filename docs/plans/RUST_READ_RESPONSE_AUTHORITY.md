# Managed read response authority

## Purpose and scope

Peel the reviewed read-only authority seam from the local ordinary-Bybit/group
follow-on onto actual #1783. A live same-command Context alone does not identify
the HTTP response being delivered. Paired reads and shared history completion
need an engine check before native-cache/Store publication or companion markers.
The first callers are BingX serial leverage/margin reads and the shared managed
Completion bridge. Joined metadata still uses the separate execution/observer
core; this seam does not introduce another waiter mechanism.

## Exact API

Add three read-only methods to the existing Cx impl in
`sdk-rs/engine/src/request_context_registry.rs`:

- `validate_http_context(&self, context: &Context) -> Result<(), Error>` checks
  current issuer, issuance, generation and raw deadline without mutation.
- `validate_read_completion_context(&self, context: &Context) -> Result<(), Error>`
  delegates existing completion validation, then requires command Read authority
  whose owner has never declared a mutation or installed a write journal. It
  preserves existing completion expiry semantics and grants no new send rights.
- `validate_read_http_delivery(&self, root: &Context, leg: &Context)
  -> Result<(), Error>` requires the original read completion root, a live read
  leg with the same owner/generation and deadline at most the root deadline,
  and the exact engine-private HTTP Delivery for that leg. An original root may
  have retired its own logical handle while its bounded child remains live.

Publish synchronously after validation and complete in the same callback, without
yielding. Native status/body proof and publication freshness remain independent
requirements. A failure callback also has delivery ownership; this validator
does not prove that a valid native response exists. Read completion cannot seal
a command that has also declared financial mutation work.

## Implementation and verification

Reuse the exact reviewed local helper bodies and real managed-delivery tests
from `b247fecc459`, without importing unrelated journal/Bybit/plan changes.
Add the dedicated test module plus the two expiry/mixed-owner registry tests.
No engine/history.rs, allocator, HTTP lifecycle, host, venue, ratchet inventory
or source of retry/completion authority changes. Add caller documentation that
describes the opt-in behavior and synchronous publication requirements.

Required controls cover actual managed preparation/binding/final claim and HTTP
delivery; a live same-owner wrong sibling and WS callback cannot publish native
or Store state. Valid delivery publishes and completes once. Wrong issuer,
generation, root, altered policy, retired leg, expiry, extended bound and mixed
Read/Mutation owner fail without mutation/actions/resource allocation. Failure
delivery remains explicitly ownership-only. Repeat checks are read-only.

Run all-feature engine tests, focused Binance/BingX compatibility, strict engine
Clippy, fmt/bans and ratchet checks; independently audit extraction and authority
boundaries. Publish this small PR while the Bybit/group implementation continues.
Respect #1784's landing priority and integrate whatever lands before final proof.

Architecture gate: No. Response and completion authority stay with the existing
engine registry; consumers receive no numeric fallback, allocator or ledger.

## Extracted checkpoint

The three method bodies, two registry tests and dedicated five-test module are
byte-exact extractions of the reviewed local child. Seven new test functions
include the sixteen-case issuer/generation/root/expiry/nonmutation matrix and
the real managed final-handoff controls above. All548 all-feature engine tests,
69 BingX account tests and22 Binance request-context tests pass. Strict all-target
all-feature engine/Binance/BingX Clippy, fmt, bans and13 ratchet controls pass.
Independent plan/extraction/source/caller audits are clean. These checks prove
the opt-in seam, not native adoption or joined-observer completion. Final CI and
proof follow the current landing queue, preserving #1784's profile work.
