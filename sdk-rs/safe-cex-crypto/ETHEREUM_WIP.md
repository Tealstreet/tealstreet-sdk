Ethereum shared operations are implemented and scoped-review MERGE after resuming
on integration 35b7ec85f9. This adds ABI parameters, Keccak, EIP-191, recoverable
secp256k1 signing, getAddress and isAddress; it does not port a venue adapter.

Oracle-first fixtures precede source implementation. The actual asterdex TypeScript
helpers supply ABI bytes, hashes and signatures, rather than a reimplemented
composition. Oversized ICAP acceptance is preserved and marked ref-bug.

Verification: 62 crypto library tests, strict all-target clippy, wasm check,
parity (6967 rows), 172 CI wiring controls and 78 layer controls passed.
All live crypto generators replayed without fixture drift. Four Ethereum
oracles also executed in a disposable tracked-source mirror without any
node_modules ancestor. Missing runtime imports and missing compiler fail
MODULE_NOT_FOUND; a changed actual TypeScript composition fails preimage drift.
The earlier eight production mutations remain the source-level controls:
ABI tuple encoding, nonce endianness, EIP-191 hashing, double signing, checksum
bypass, ignored address refusal, unknown domain acceptance and duplicate fields.

Integration merge CI counts: 56 / 70 parents -> 70, zero named steps missing.
The installed Yarn binary was invoked directly for the known Volta shim loop.

Remaining EIP-712 bool and bytes-array coercion findings stay disclosed on
signInner. This is scoped operations evidence, not a whole-SDK parity claim.
