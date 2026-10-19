# Unused signer caller census

Caller census baseline: mirror master `5368a1491c`, rechecked after normally
merging master `2a9ed2be9a` before deletion. No tracked shipped caller was added.

The maintained Lighter exchange imports LighterApiClient, OrderApi and
CandlestickApi. Its signer adapter and builder approval import TransactionClient.
The adapter constructs TransactionClient directly. No tracked application or
venue imports TransactionSigner, SignerError or createSigner. The SDK still
exports them through its signer barrel and default Signer/createSigner fields;
those declarations, SDK tests and documentation are not shipped call sites.

A TypeScript-AST import census over 948 tracked application/package source files
finds exactly four SDK imports, all named imports from its root barrel:

| Consumer | Imported values |
| --- | --- |
| lighter.api.ts | LighterApiClient |
| lighter.builder-approval.ts | LighterApiClient, TransactionClient |
| lighter.exchange.ts | CandlestickApi, LighterApiClient, OrderApi |
| lighter.signer.ts | LighterApiClient, TransactionClient |

All four live under `packages/safe-cex/src/exchanges/lighter/`. There is no
default/namespace SDK consumer that could reach its Signer/createSigner fields.
Identifier and relative-import searches also find no application/venue caller
of the retired signer, key-manager or transaction-hasher modules. Rust searches
over tracked sources find only the SDK declarations, exclusive tests and the
live SDK client binding; that binding imports KeyFuture rather than KeyHost.

The Rust signing_runtime module has no production SigningHost implementation.
Its consumers are the SDK signing_runtime tests. signing_types is consumed only
by SDK key_runtime tests. The PoseidonKeyHost adapter has only its own test
consumer. The venue sdk_key_management module is consumed only by
tests/live_key_management.rs. The unrelated normalizeSignerErrorMessage exchange
method remains live and must not be removed by a name-substring search.

The dependency tail identified for removal is signing_runtime, signing_types,
the unused PoseidonKeyHost adapter, key-manager orchestration/primitive helpers,
and the old standalone transaction hash encoders. Their exclusive tests,
source-oracle fixtures, mutation controls and policy entries also need removal.
KeyFuture remains live in api_reads, client_runtime, http_types and the venue's
SDK binding. EncodingError and bigint_string in transaction_encoding remain
dependencies of amount_encoding; its utility tests and controls must survive.
The canonical crypto package has independent loader/example consumers, so its
implementation is outside this signer-only removal.

The unused modules and exclusive fixture sections are deleted, including the
SDK crypto dependency and its lockfile edge. KeyFuture remains in key_runtime;
transaction_encoding retains only EncodingError and bigint_string for amount
utilities. TS declarations remain unchanged and all extracted rows remain visible
with empty Rust targets and caller-proof not-applicable notes. Matching dead
exemptions and accepted contracts are removed without relaxing any scanner.

Verification covers extractor check, retained SDK and real binding targets, full
parity index, all-features cancellation/override, fmt, strict touched-crate Clippy
and CI wiring. This document records the caller rationale; execution receipts
belong in the PR description after the gates run.
