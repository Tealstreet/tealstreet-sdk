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
preflights remain mandatory before nonce/signing/submission, and signed attributes
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
