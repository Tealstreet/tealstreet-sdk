# Unused signer caller census

Inspection baseline: mirror master `5368a1491c`. No deletion or build has been
performed in this cohort yet.

The maintained Lighter exchange imports LighterApiClient, OrderApi and
CandlestickApi. Its signer adapter and builder approval import TransactionClient.
The adapter constructs TransactionClient directly. No tracked application or
venue imports TransactionSigner, SignerError or createSigner. The SDK still
exports them through its signer barrel and default Signer/createSigner fields;
those declarations, SDK tests and documentation are not shipped call sites.

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

Resume by trimming these modules and exclusive test sections, removing the SDK's
unused crypto dependency, and updating Cargo.lock. Keep all extracted TS symbols
visible and give retired mappings an explicit caller-proof note with empty Rust
targets. Remove only matching dead exemptions/contracts. Then run extractor
check, retained SDK/real binding targets, full parity index, all-features
cancellation/override, fmt, strict touched-crate Clippy and CI wiring checks.
