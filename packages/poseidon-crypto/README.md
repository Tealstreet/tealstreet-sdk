# @tealstreet/poseidon-crypto

WebAssembly-powered Poseidon2 cryptography library for TypeScript/JavaScript. This package provides Poseidon2 hash functions and Schnorr signatures over an elliptic curve (ECgFp5).

## Installation

This package is part of the Tealstreet monorepo. It's available as a workspace package:

```typescript
import { initWasm, getCryptoApi } from '@tealstreet/poseidon-crypto';
```

## Quick Start

```typescript
import { initWasm, getCryptoApi } from '@tealstreet/poseidon-crypto';

// Initialize WASM (call once at app startup)
await initWasm();

// Get the crypto API
const crypto = getCryptoApi();

// Hash data
const hash = crypto.poseidon2Hash('0000000000000000000000000000000000000000000000000000000000000000');
console.log('Hash:', hash);

// Generate keys and sign
const keys = crypto.schnorrGenerateKeys();
const signature = crypto.schnorrSign('0000000000000000000000000000000000000000000000000000000000000001', keys.privateKey);
const isValid = crypto.schnorrVerify('0000000000000000000000000000000000000000000000000000000000000001', keys.publicKey, signature);
console.log('Valid:', isValid);
```

## Development

### Building WASM

To rebuild the WASM binary from the Go source:

```bash
cd packages/poseidon-crypto/src/wasm
GOOS=js GOARCH=wasm go build -o poseidon_crypto.wasm main.go
```

### Running Tests

```bash
# Run TypeScript tests
yarn workspace @tealstreet/poseidon-crypto test

# Run Node.js WASM tests
cd packages/poseidon-crypto/src/wasm
node test_node.js

# Run cross-validation
node validate_wasm.js
```

### Running the Browser Example

```bash
cd packages/poseidon-crypto/src/wasm
npx serve .
# Then open http://localhost:3000/example.html
```

## JavaScript API

### Poseidon2 Hash Functions

#### `poseidon2Hash(input)`

Hash arbitrary data using Poseidon2.

- **Parameters:**
  - `input` (string): Hex-encoded input data (must be multiple of 8 bytes = 16 hex chars)
- **Returns:** Hex-encoded hash output (32 bytes = 64 hex chars)

**Example:**
```javascript
const hash = poseidon2Hash("0000000000000000000000000000000000000000000000000000000000000000");
console.log(hash); // 32-byte hash as hex string
```

#### `poseidon2HashTwoToOne(hash1, hash2)`

Combine two hash outputs into one.

- **Parameters:**
  - `hash1` (string): First hash (32 bytes = 64 hex chars)
  - `hash2` (string): Second hash (32 bytes = 64 hex chars)
- **Returns:** Combined hash (32 bytes = 64 hex chars)

**Example:**
```javascript
const combined = poseidon2HashTwoToOne(
  "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
);
```

### Schnorr Signature Functions

#### `schnorrGenerateKeys()`

Generate a new Schnorr key pair.

- **Returns:** Object with `privateKey` and `publicKey` (both 40 bytes = 80 hex chars)

**Example:**
```javascript
const keys = schnorrGenerateKeys();
console.log("Private key:", keys.privateKey);
console.log("Public key:", keys.publicKey);
```

#### `schnorrPkFromSk(privateKey)`

Derive public key from private key.

- **Parameters:**
  - `privateKey` (string): Private key (40 bytes = 80 hex chars)
- **Returns:** Public key (40 bytes = 80 hex chars)

**Example:**
```javascript
const publicKey = schnorrPkFromSk("0123456789abcdef...");
```

#### `schnorrSign(message, privateKey)`

Sign a message.

- **Parameters:**
  - `message` (string): Message to sign as hex (must be multiple of 8 bytes)
  - `privateKey` (string): Signing key (40 bytes = 80 hex chars)
- **Returns:** Signature (80 bytes = 160 hex chars)

**Example:**
```javascript
const signature = schnorrSign(
  "0000000000000000000000000000000000000000000000000000000000000001",
  keys.privateKey
);
```

#### `schnorrVerify(message, publicKey, signature)`

Verify a signature.

- **Parameters:**
  - `message` (string): Original message as hex
  - `publicKey` (string): Signer's public key (40 bytes = 80 hex chars)
  - `signature` (string): Signature to verify (80 bytes = 160 hex chars)
- **Returns:** `true` if valid, `false` otherwise

**Example:**
```javascript
const isValid = schnorrVerify(
  "0000000000000000000000000000000000000000000000000000000000000001",
  keys.publicKey,
  signature
);
console.log("Valid:", isValid);
```

## Data Formats

### Input Requirements

- **Hash inputs:** Must be hex-encoded and a multiple of 8 bytes (16 hex characters)
- **Private keys:** 40 bytes (80 hex characters)
- **Public keys:** 40 bytes (80 hex characters)
- **Signatures:** 80 bytes (160 hex characters)
- **Hash outputs:** 32 bytes (64 hex characters)

### Example Data

```javascript
// Valid 8-byte (16 hex chars) input for hashing
const input = "0000000000000001";

// Valid 32-byte (64 hex chars) input for hashing (4 field elements)
const input32 = "0000000000000000000000000000000000000000000000000000000000000000";

// Generate keys
const keys = schnorrGenerateKeys();
// keys.privateKey: 80 hex chars
// keys.publicKey: 80 hex chars

// Sign and verify
const msg = "0000000000000000000000000000000000000000000000000000000000000001";
const sig = schnorrSign(msg, keys.privateKey); // 160 hex chars
const valid = schnorrVerify(msg, keys.publicKey, sig); // true/false
```

## Error Handling

All functions may return an object with an `error` property if something goes wrong:

```javascript
const result = poseidon2Hash("invalid");
if (result.error) {
  console.error("Hash failed:", result.error);
} else {
  console.log("Hash:", result);
}
```

## Testing

### Running Tests

The WASM module includes comprehensive tests:

```bash
# Run Node.js tests
node test_node.js

# Run cross-validation (verifies WASM matches native Go)
node validate_wasm.js
```

### Cross-Validation

The WASM implementation has been validated against the native Go implementation to ensure identical results:

- ✓ `poseidon2Hash` - Produces identical hashes
- ✓ `poseidon2HashTwoToOne` - Produces identical combined hashes
- ✓ `schnorrPkFromSk` - Derives identical public keys
- ✓ `schnorrVerify` - Validates signatures correctly

Test vectors are generated from the native Go implementation (`validation_tool.go`) and validated against WASM (`validate_wasm.js`). All tests pass, confirming the WASM bindings faithfully implement the cryptographic operations.

## Browser Compatibility

Requires a modern browser with WebAssembly support:
- Chrome 57+
- Firefox 52+
- Safari 11+
- Edge 16+

## Security Notes

- This is cryptographic software. Ensure you understand the security implications before using in production.
- Private keys are generated using `crypto/rand` in Go, which should provide cryptographically secure randomness.
- The signature scheme uses Schnorr signatures over an elliptic curve (ECgFp5) with Poseidon2 hashing.

## License

See the main repository LICENSE file.
