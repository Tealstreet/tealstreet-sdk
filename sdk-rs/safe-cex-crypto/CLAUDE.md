# Crypto primitives

Behavioral references remain the TypeScript exchange docs and the existing
[Go WASM wrapper](../../packages/poseidon-crypto/CLAUDE.md); do not copy their specs.

**Oracle first.** Lighter fixture capture executes the checked-in Go WASM without
rebuilding it, records artifact hashes and entropy, and refuses overwriting an
existing fixture. Rust mismatches are findings, never reasons to change a golden.
Other scheme oracles execute the repository's JS dependencies or npm WASM.
Every new scheme test enters through raw JSON text, including overflow tokens.

**Caller-owned preimages.** These modules contain primitives, not adapters.
EIP-712 hashes its declared domain/type. Stark accepts a precomputed hash;
zkLink accepts serialized binary bytes, with a 92-byte bound. JSON envelope
context is not automatically authenticated. MessagePack implements JS Number
and Object.keys ordering, including integer-index keys; it cannot directly
serialize arbitrary-precision serde numbers as their private representation.

`es256::Es256SigningKey` imports P-256 PKCS8 DER and signs exact caller bytes
with SHA-256, returning 64-byte IEEE-P1363 `r || s` as WebCrypto/JWS require.
PEM normalization, JWT claims and key caching stay with the venue. RFC6979
nonces are deterministic; WebCrypto chooses random nonces, so valid signature
bytes can differ. Tests pin the RFC6979 known answer and verify signatures in
both directions through the live Coinbase TypeScript/WebCrypto signer. Key
handles have no serializer, Debug implementation or private-key export.

**Lighter semantics.** Raw input words are little endian. `poseidon2_hash` and
`poseidon2_hash_fp5` reproduce Go's noncanonical reduction.
`poseidon2_two_to_one` accepts only canonical hash outputs; unlike Go it refuses
noncanonical words, a fail-closed bound pinned against Go's actual hashes.
Today's callers supply canonical hash outputs to that compression path; raw messages hash to Fp5 before Schnorr. Private-key inputs reduce modulo the group order. Nonces are canonical,
nonzero and consumed by signing. Hosts generate fresh secret CSPRNG bytes and
handle entropy failures; never reconstruct a nonce from previously used bytes.

**Required serialization features.** arbitrary_precision prevents numeric
parser refusal and is explicitly tested. preserve_order prevents signed data
being re-keyed; each scheme tests raw two-key wire order and signature-last,
and the transmitted authenticated input is checked against its signing preimage.
The output-format match remains exhaustive with no wildcard.

Encoding and WsOutputFormat represent the maintained inline string unions.
Their concrete deserializers admit String before private Serde remote enums;
valid tagged maps are not source inputs. Serialize metadata and native variants
remain unchanged. The string_enum_ingress target checks all nine variants and
raw/owned/borrowed/qualified refusals. scripts/string-enum-controls.py removes
each frontdoor independently, requires a compiled named tagged-map failure,
then restores exact bytes and requires both tests to pass.

See [GATE_EVIDENCE.md](GATE_EVIDENCE.md) for actual mutation outcomes and any
pending integration gates; a fixture count alone does not prove corpus coverage.

**Deliberate dependency ruling (October 2026).** poseidon-hash 0.1.4 and
goldilocks-crypto 0.1.2 are UNAUDITED, with sole registry publisher Bvvvp009,
one contributor and three upstream commits, stale since March 2026. The pair
was published 68 seconds apart. They are pinned deliberately with exact `=`
versions and Cargo.lock. The Go vector harness is the oracle; vector agreement
is not an audit claim. It must run in CI through root `yarn crypto:lighter:check`
and the mandatory rust.yml step, protected by scripts/rust-ci-wiring.test.mjs.

Deferred option: vendor both crates into `thirdparty/` to remove their external
supply-chain surface, at the cost of owning approximately 5,000 lines of
cryptography. This is a named future choice, not part of P5a.

Lighter nonce ownership: the host supplies fresh secret CSPRNG bytes and handles
entropy errors. `SchnorrNonce::from_bytes` rejects zero and noncanonical scalars;
`schnorr_sign` consumes the private, non-Clone/non-Copy nonce type. This crate
owns validation and the signing boundary, not a global/browser entropy source.
Never reconstruct a consumed nonce from reused bytes. Verification rejects a
neutral nonce commitment; the committed Go zero-nonce signature is deliberately
invalid and demonstrates full secret-scalar recovery from public `(s,e)`.

**Host nonce contract.** Generate fresh secret CSPRNG bytes for EVERY signature.
Never derive a nonce deterministically and never retain, log, serialize or cache
raw nonce bytes. Handle entropy failure as an error; do not sign. Uniform 40-byte candidates are 320 bits while the group order is 319 bits:
measured rejection is approximately 50%. A rejected candidate is NOT an entropy
provider failure: mandatory rejection sampling draws fresh bytes and retries.
An actual CSPRNG error stops signing. Zeroize the host buffer immediately on
both successful and failed construction. The borrowed input cannot be wiped by
this crate; every caller owns that obligation. Consume the nonce exactly once.

`SchnorrNonce` zeroizes its owned bytes on drop (including signing errors), using
volatile stores/compiler fences from zeroize. This is useful on native and wasm
linear memory, not complete erasure: compiler temporaries, dependency scalar
copies, host RNG storage and browser/OS snapshots remain outside this guarantee.

A host-owned rejection-sampling example (the crate provides no RNG):

```rust
use safe_cex_crypto::lighter::SchnorrNonce;
use zeroize::Zeroize;
fn sample(
    mut fill: impl FnMut(&mut [u8; 40]) -> Result<(), String>,
) -> Result<SchnorrNonce, String> {
    for _ in 0..64 {
        let mut bytes = [0; 40];
        let entropy = fill(&mut bytes);
        if let Err(error) = entropy {
            bytes.zeroize();
            return Err(error);
        }
        let candidate = SchnorrNonce::from_bytes(&bytes);
        bytes.zeroize();
        if let Ok(nonce) = candidate {
            return Ok(nonce);
        }
    }
    Err("CSPRNG produced no valid nonce in 64 fresh draws".into())
}
```

The bound prevents a broken zero-producing provider from hanging; a healthy
uniform provider exhausts it with probability approximately 2^-64. Never retry
with retained bytes. The live Node CSPRNG host-contract test measures one-shot
rejection and requires the looping host to satisfy every signing request.

**Ethereum primitives are shared and venue-free.** `ethereum.rs` holds
`get_address`/`is_address` (ethers semantics: `getAddress` normalizes hex and
direct-mode ICAP and ENFORCES mixed-case EIP-55, while `isAddress` takes a
`serde_json::Value` because ethers' version never throws and is handed
non-strings), `keccak256`, the EIP-191 message hash over exact bytes rather than
a hex spelling, and RFC6979 recoverable `sign_digest` over an already-prepared
digest. There is no venue payload construction and no entropy here.
**Checksum enforcement reaches the whole typed message**: `eip712.rs` walks every
declared `address` field recursively — through arrays and nested structs, bounded
at 128 levels — plus the domain's `verifyingContract`, so one bad checksum
anywhere rejects the signature instead of signing a normalized-elsewhere value.
ABI encoding is deliberately one NAMED fixed shape
(`encode_string_address_address_uint256`), not a general encoder: a venue needing
another shape adds another named function, which keeps the auditable surface
enumerable.

Canonical remote-signer WS records live in
[`safe-cex-types::ws_auth`](../safe-cex-types/src/ws_auth.rs) and are re-exported
here. `extraData` stays raw JSON: preserving it must not introduce a parser
depth/number refusal in metadata the signer does not inspect. Signing-field
parser limits and correlated malformed-input refusals remain separately tested.

**Field identity evidence.** Original feeder artifacts remain byte-identical; field-identity additions are separately labeled self-authored supplements and inherit no original independence claim. The Stark domain supplement differentiates version2/3 from revision1 through the actual SDK and public raw hash caller; a compiling version/revision swap must fail by name. Crypto consumes serialized Lighter/zkLink preimages, not their TransactionClient/ContractBuilder fields. Coincident SDK metadata dimensions remain explicit builder-owner blind spots, never proof of native field-construction parity.

**Enum payload cardinality.** Stark guards require exact declared enum tuple arity before visiting any consumed leaf. The SDK ignores extra elements, while the native encoder already refused them; that refusal remains CHOSEN. Empty tuple encoding retains the SDK parser's extra zero word; partial empty slots retain their original positions and ignore the corresponding supplied value. Unused variant tokens remain authenticated without evaluating their subtype. Only metadata and ignored value copies are normalized; native enum/composite composition uses the original raw revision-specific type signatures, UTF-16 dependency ordering and revision-1 SDK preset overlay. Arrays and Merkle leaves recursively retain those hashes; the SDK's single-leaf Merkle root remains that leaf. The twelve-case actual-SDK cardinality probe predates the repair and original fixtures stay unchanged.

Poseidon loader interface records have strict object decoders and required
strings. SchnorrKeyPair does not enforce the source comments' key lengths or
hex formatting; WasmError carries the string verbatim. Native field identities
precede wire checks, and encoded lone-unit controls preserve size while changing
bytes so refusal of both inputs cannot hide a stale UTF16 witness. These records
do not implement WASM loader state or random key generation.
The mandatory `crypto:oracles:check` runner includes `live_poseidon_types` with
`--include-ignored`, executing both native record identity and the encoded-content
mutation control after the existing signing oracles and fixture drift check.

Stark felt/shortstring inputs normalize numeric strings with shared `js_bigint`
before the existing prime-range guard. Failed numeric conversions retain the
original shortstring. The live `stark_numeric_strings` witness pins both SNIP-12
revisions, alternate radices, JavaScript whitespace and nested/domain refusals.

`parse_felt` validates original account/key strings before native field parsing.
For reordered revision-0 domains, metadata uses a canonical copy while encoding
retains the original three-field definition and type hash. Six source domain
permutations and the actual Paradex caller payloads pin the authenticated order.

Stark raw JSON numeric leaves use shared `js_bigint`; the owner ruling is
[the numeric ingress defect](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).
Declared struct/preset fields are projected before native value decoding.
Native i128 normalization follows raw enum tuple and Merkle leaf references,
and recursive composition retains SDK revision-0 felt coercion and original
type signatures. The 401-case `stark_numeric_json` witness preserves raw input
tokens and independently pins numeric, coercion and recursive inventories.

Selector encoding preserves the SDK's named UTF-8 Keccak preimage, including
its default-name hashes, separately from numeric hex selectors. Revision 0
checks original hex range; revision 1 uses the native Felt all-byte constructor
for the SDK's Poseidon reduction. `stark_selectors` pins 48 source cases; lone
units cite the canonical UTF-8 boundary in the witness.

Stark shortstrings retain SDK unpadded ASCII nibbles and LF/CR behavior.
Revision-1 strings build the original 31-unit ByteArray frame before native
Poseidon composition; `stark_text_encoding` pins 110 source cases.

Enum ingress selects only the first enumerable source entry, including numeric
key ordering and indexed object/string tuple payloads. Ignored entries are
projected before native decoding; array tuple cardinality remains explicit.
Selector coercion uses shared `js_string`. Domain revision recognition precedes
all value normalization and projection; `stark_domain_revision` pins 30 cases.

MessagePack's native encode_js_value accepts the existing JsJsonValue carrier.
Prepared f64 NaN/infinities survive authentication before JSON wire projection;
undefined is nil, integer encoding and own-key ordering match the raw entrypoint.

The raw MessagePack adapter uses retained JSON capture before conversion to the
native carrier. A literal private-Number-looking object remains an object;
its native member assertion precedes the source byte golden.

**zkLink contract capability.** `zklink::contract` implements the imported
ContractBuilder serialization and the Ethereum-signature branch of
JsonRpcSigner for contracts. The venue retains validation, wasm argument
coercion and transaction orchestration. The seed handle owns zeroizing65-byte
storage with no Debug/serde/export; v is consumed verbatim as the npm SDK does.
It exposes no wallet provider, seed acquisition or other transaction signing.

Contract preimages retain the SDK's low16 slot, low24 nonce and low8 pair,
40-bit decimal-float amount, 120-bit price, direction, distinct fees and subsidy.
Wire size/price remain the original numeric values as canonical decimal strings;
packing affects the authenticated preimage only. `closest_packable_amount/fee`
select the closest lower encoding, including the preceding exponent's maximum
mantissa, rather than always truncating at the next exponent.

Chosen bounds: decimal conversion beyond u128, prices beyond120 bits and inputs
whose SDK packing product overflows or whose exponent exceeds31 return errors.
These replace source panic paths; no identical error-text claim is made. Inputs
are already typed after WASM conversion; native JSON u32/string refusals do not
claim to reproduce JavaScript's modulo conversions. Existing primitive fixtures
remain unchanged. The separately labeled24-contract npm supplement plus18
rounding cases pin fields and wire signatures, with immutable artifact hashes.
The native Contract JSON boundary accepts named objects through a private remote
companion and the shared object guard. Complete positional arrays are rejected;
the post-WASM field types and authenticated byte construction stay the same.

The Lighter reviewed shared-types provenance guard hashes the five runtime enum
declarations consumed by emitted `lighter.types.ts`, with canonical TypeScript
printing. The pin-name census must equal the actual value-import references.
Unrelated erased declarations can change; missing pins/declarations and changed
enum values fail named controls. Fresh captures still verify their recorded full
source digest before historical provenance normalization. Original generators,
92-case fixtures, preimages and 24-case field-identity expectations stay pinned.
