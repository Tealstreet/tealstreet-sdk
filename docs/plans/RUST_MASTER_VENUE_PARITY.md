# Hyperliquid and Lighter SDK-master parity

Plan, lane codex-zs5e83, 2026-10-23. Base is the exact reviewed master-integration
PR #1869 head `cc0d25e820ab5d6fec98066d54b59b381f809c8f`. Deliver this branch
to claude-4yjlw0 for normal merge into #1869; no independent landing merge.
Keep every incoming TS hot-patch, tape and harness byte unchanged.

## Scope and source audit

Master `dd9cae4a418` makes Hyperliquid userFees use vault, then applicationId,
never the API-agent key; public-only or unresolved identity keeps the existing
0.00045 taker / 0.00015 maker fallback without HTTP. Rust account_read already
uses that trading identity and private startup resolves agents before fees.
Its remaining mismatch is the public/anonymous startup branch issuing an empty
userFees read. Remove that read and retain default market fees. Add a real-engine
fee-subject control for applicationId, vault precedence and discovered agent
identity, plus assertion-RED public/anonymous no-I/O controls. Preserve private
bootstrapping, role discovery and all other REST requests. No new identity helper.

Master `21644979020` configures Robinhood integrator47296. Match both Robinhood
deployments; explicit integrator override still wins, ordinary mainnet keeps
158385, ordinary testnet remains without a default. Existing tier and approval
preflights are attempted before nonce/signing/submission; preserve their native
best-effort lookup-failure continuation and parsed-invalid/405/429 blocking.
Signed attributes
must be exactly {1:47296,2:250,3:0}. The #1869 harness already approves that exact
identity. Update the old Robinhood skip-attribution assertion to this new master
contract, add accepted native signed-attribute and explicit-override controls.

Master `4eca3e4139c` reserves one positive48-bit client index per placement leg,
including failed/unknown sends, split/bracket legs and batch/single commands.
Rust already calls the one mint_id for every prepared leg and retains reservations
across failures, but seeds only once and wraps at exhaustion. Keep that existing
instance state and call graph; recompute the timestamp+entropy candidate on each
reservation and choose max(candidate,last+1), rejecting above (1<<48)-1 without
wrap/reuse. Use wider checked arithmetic for timestamp*100+jitter modulo the
protocol maximum. This is the protocol client-order index, not the transaction
nonce. Add original-RED boundary/recomputed-candidate unit controls through shared
test_support::ProbeVenue; keep native split/bracket/batch and failure controls.
No transaction nonce, signer, command ownership, retry or status policy changes.

## Ownership and inventory

Fresh all-open file inspection finds Lighter trading.rs held by #1866. Nonce
owner claude-m26ps4 explicitly authorizes the independent builder and mint_id
regions and promises not to edit them; its held struct/nonce/sign-loop regions
remain unchanged. Lighter lib.rs is held by #1875 and is not edited here.
Hyperliquid account.rs and the selected tests are unheld; leave held sign/trading,
history/spot and request_context_candles.rs alone. Report any further collision.
The complete venue inventories remain in their existing RequestContext plans.
This parity slice changes none of the counted request/completion call sites;
retain the entire incoming legacy manifest verbatim and run both ratchets.

## Execution and proof

Commit this plan, independently review source/adversarial, caller and incoming-map
preservation, then commit assertion-RED controls before production. Implement only
account.rs and trading.rs protocol differences, preserve native assertions except
the intentionally changed master contract, and append concrete CALLERS/AUDITS/
QUALITY receipts. Run Lighter+Hyperliquid+Binance native tests with two build jobs,
strict touched-crate clippy/fmt, bans and both request-context checks. Repeat all
three reviews on the frozen head and hand it to #1869's owner. CI supplies the two
parity replays and broad WASM proof; do not locally build release WASM or run
replay/timing sweeps. Do not re-tape unless a demonstrated TS-reference change
requires it; the supplied #1869 harness already reflects Robinhood attribution.
Architecture gate: No; existing venue protocol identity/index encoding is aligned
with master through existing helpers, with no new cross-cutting mechanism.

Review correction: Rust's existing user() must also skip empty vault and
applicationId strings, matching TypeScript's `vault || applicationId` truthiness.
Normalize only emptiness (never trim or reinterpret the address) in that existing
selector; do not introduce a fee-only parallel selector. Empty vault falls back
to a valid applicationId; empty applicationId is unresolved and must not produce
an empty-subject fee read. Existing role discovery can establish a real trading
identity before private bootstrap. Since other account reads share user(), they
also use the nonempty trading identity rather than an empty vault. Add explicit
empty-vault fallback and empty-applicationId/no-premature-fees controls. This is
necessary for the requested master fee-subject parity and is included in CALLERS.

Existing native compatibility update: fees.rs's anonymous control previously
expected one empty-subject userFees request. Change only that assertion to no
fee request, retaining its loaded-markets/no-role/no-private-mode assertions and
all five private identity/failure/timer controls. This is the same intentional
master contract change reproduced by the new no-I/O RED test. Also retain the
zero-candidate control after correcting its public entropy-module import in
0f0e92e553c; the corrected original unit run is2 assertion-RED +1 compatibility
GREEN, not a compiler failure counted as RED.

Frozen source receipt e71a0ff95f76 genuinely integrates actual258c and owner2a04
(Robinhood testnet harness). Source/adversarial review is CLEAN. Caller review
corrected an overstatement of existing approval policy: lookup failures can
continue best-effort; this slice does not change it. The final docs-only correction
preserves all runtime/tests/manifests. Before integration689 native tests in61
groups and strict checks passed; current integrated gates supply final evidence.

## Phase 2: frozen Robinhood fixture provenance

CI run39953255157 on1845feb21ee proves Hyperliquid parity passes. Its Binance job
fails one real-WASM boundary test: the test account has no approved integrator,
so the newly attributed Robinhood placement correctly refuses. Its other700
boundary tests pass. Add only a venue-selected approved integrator to the existing
recorded account (47296 for Robinhood,158385 for plain Lighter); keep all native
signer-index, slippage, settings, tier refusal and delegated signer assertions.
Add an explicit signed L2TxAttributes assertion for Robinhood so the fixture
cannot silently disable attribution. Use the existing override/default when
choosing the test approval and leave production transport/signing untouched.

The same run's Robinhood public report has186 recognized declared diagnostics.
Its two private reports have exactly the eleven already declared paths each,
but tapes changed legitimately with master21644979020's integrator and the
reviewed1869 harness47296 updates, invalidating the old content hashes. Compare
all exact observed diagnostic keys and run full assessGateReport on the CI reports
before and after a proposed refresh. Change only the two existing Robinhood
private tapeSha256 strings; retain every diagnostic/key/rationale and every other
venue/public entry verbatim. Neither add nor broaden an allowance. Keep the
strict raw reports FAIL; a known-failure result is not complete parity.

Base: #1869 reviewed1845feb21ee plus genuine current landing9aef. Frozen report
artifact: strict-replay-lighter-robinhood from39953255157. Independently verify
mainnet hash173e43cc71f0c6b317a6001ababf920307df3e46da9e6f5e412e3fa7505fb9e0
and testnet4850485f3abcc1e1bb126a9c7d6c36bea528be3c42115dab187d3f98f0eeee10.
Trace both hashes to the deterministic tape generator; compare old/new decoded
tape data to prove only the authorized integrator fixture fields differ. Use
existing policy and verifier controls; no local replay/performance/release build.
For the WASM RED/GREEN receipt reuse CI's matching engine artifact and run the
one boundary test file, strict TS lint/typecheck and both request-context checks.

Commit plan, review it independently, then commit failing fixture/provenance
controls before the narrow fix. Files: existing WASM boundary test, two hash
fields in known-gate-failures.json, this plan and caller/quality appendices as
needed. Fresh all-open source-file lists show neither fixture nor declaration
file held. All Rust sources, raw tapes, generator, transport, financial fields
and incoming legacy manifest stay exact. Deliver independently reviewed head to
claude-4yjlw0 to fold into1869; no direct landing merge. Architecture gate: No,
existing test data and exact content identity refresh, with no new gate policy.

Phase2 RED receipt: `d149360a455` adds the explicit signed attribute control;
using frozen CI39953255157's authenticated Light family artifact (WASM SHA256
f358f259f7b1d7fba3b905e48f106f6937ac379fcc74e8a53b25e1ffb33a9c59), the
unchanged test account reproduces1 approval refusal and21 passing cases. This is
a scoped native fixture check, not a whole-current-package/replay certification.
Independent and primary VM generator runs reproduce both old/new content hashes
and prove37 records each, only10 authorized decoded fields changed across9
records (three approval account indices, seven native attribute objects). Full
policy assessment of the real CI reports is22 unexpected before refresh and
zero errors afterward, with11 existing known keys each and186 public known keys.
No source/generator/tape/diagnostic/rationale changes are part of the hash refresh.

Phase2 local gates: the same authenticated CI family module gives22/22 real-WASM
boundary cases GREEN after the fixture correction. Nine existing policy/verifier
controls pass, as do safe-cex typecheck, targeted non-ignored ESLint, bans and both
request-context checks/14 controls. Original typed refusal and existing assertions
remain covered. No Rust build/replay/performance result is inferred from this
fixture proof; #1869's fresh exact-head CI remains the broad merge gate.
