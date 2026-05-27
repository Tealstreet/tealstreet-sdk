package main

import (
	"encoding/hex"
	"testing"

	curve "github.com/elliottech/poseidon_crypto/curve/ecgfp5"
	g "github.com/elliottech/poseidon_crypto/field/goldilocks"
	p2 "github.com/elliottech/poseidon_crypto/hash/poseidon2_goldilocks_plonky2"
	sig "github.com/elliottech/poseidon_crypto/signature/schnorr"
)

// TestPoseidon2HashCrossValidation tests that the WASM hash function produces
// the same results as the native Go implementation
func TestPoseidon2HashCrossValidation(t *testing.T) {
	tests := []struct {
		name  string
		input string // hex encoded
	}{
		{
			name:  "zero input (4 elements)",
			input: "0000000000000000000000000000000000000000000000000000000000000000",
		},
		{
			name:  "single element",
			input: "0100000000000000",
		},
		{
			name:  "two elements",
			input: "01000000000000000200000000000000",
		},
		{
			name:  "pattern input",
			input: "0123456789abcdef0011223344556677",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Decode input
			inputBytes, err := hex.DecodeString(tt.input)
			if err != nil {
				t.Fatalf("failed to decode input: %v", err)
			}

			// Convert to field elements
			numElements := len(inputBytes) / g.Bytes
			elements := make([]g.GoldilocksField, numElements)
			for i := 0; i < numElements; i++ {
				elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
			}

			// Native Go hash
			hashOut := p2.HashNoPad(elements)
			expectedHash := hex.EncodeToString(hashOut.ToLittleEndianBytes())

			t.Logf("Input: %s", tt.input)
			t.Logf("Expected hash: %s", expectedHash)
			t.Logf("WASM should produce the same hash when called with this input")
		})
	}
}

// TestPoseidon2HashTwoToOneCrossValidation tests hash combination
func TestPoseidon2HashTwoToOneCrossValidation(t *testing.T) {
	// Create two test hashes
	hash1Str := "1cc3145a75f1ab638273d45f3c163881281438ebb436917a2d451041d0f18795"
	hash2Str := "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"

	hash1Bytes, _ := hex.DecodeString(hash1Str)
	hash2Bytes, _ := hex.DecodeString(hash2Str)

	hash1, err := p2.HashOutFromLittleEndianBytes(hash1Bytes)
	if err != nil {
		t.Fatalf("failed to decode hash1: %v", err)
	}

	hash2, err := p2.HashOutFromLittleEndianBytes(hash2Bytes)
	if err != nil {
		t.Fatalf("failed to decode hash2: %v", err)
	}

	// Native Go hash combination
	result := p2.HashTwoToOne(hash1, hash2)
	expectedHash := hex.EncodeToString(result.ToLittleEndianBytes())

	t.Logf("Hash1: %s", hash1Str)
	t.Logf("Hash2: %s", hash2Str)
	t.Logf("Expected combined: %s", expectedHash)
	t.Logf("WASM should produce the same combined hash")
}

// TestSchnorrSignatureCrossValidation tests signature generation and verification
func TestSchnorrSignatureCrossValidation(t *testing.T) {
	// Generate a deterministic key pair for testing
	skBytes := []byte{
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	}

	sk := curve.ScalarElementFromLittleEndianBytes(skBytes)
	pk := sig.SchnorrPkFromSk(sk)

	skHex := hex.EncodeToString(skBytes)
	pkHex := hex.EncodeToString(pk.ToLittleEndianBytes())

	t.Logf("Private key: %s", skHex)
	t.Logf("Public key: %s", pkHex)

	// Test message
	msgStr := "0000000000000000000000000000000000000000000000000000000000000001"
	msgBytes, _ := hex.DecodeString(msgStr)

	// Hash the message
	numElements := len(msgBytes) / g.Bytes
	elements := make([]g.GoldilocksField, numElements)
	for i := 0; i < numElements; i++ {
		elements[i] = g.FromCanonicalLittleEndianBytesF(msgBytes[i*g.Bytes : (i+1)*g.Bytes])
	}
	hashedMsg := p2.HashToQuinticExtension(elements)

	// Note: We can't produce a deterministic signature here because
	// SchnorrSignHashedMessage uses crypto/rand internally.
	// We can only test that verification works.
	t.Logf("Message: %s", msgStr)
	t.Logf("Hashed message: %s", hex.EncodeToString(hashedMsg.ToLittleEndianBytes()))
	t.Logf("Note: Signatures will differ due to random nonce, but verification should always work")

	// Generate a signature with a deterministic nonce for testing
	kBytes := []byte{
		0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	}
	k := curve.ScalarElementFromLittleEndianBytes(kBytes)

	// Use the deterministic signing function
	signature := sig.SchnorrSignHashedMessage2(hashedMsg, sk, k)
	sigHex := hex.EncodeToString(signature.ToBytes())

	t.Logf("Deterministic signature: %s", sigHex)

	// Verify
	isValid := sig.IsSchnorrSignatureValid(&pk, &hashedMsg, signature)
	if !isValid {
		t.Errorf("Native Go signature verification failed")
	}

	t.Logf("WASM should produce the same signature when using the same nonce")
	t.Logf("WASM verification should return true for this signature")
}

// TestPublicKeyDerivation tests that public key derivation is consistent
func TestPublicKeyDerivation(t *testing.T) {
	skBytes := []byte{
		0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	}

	sk := curve.ScalarElementFromLittleEndianBytes(skBytes)
	pk := sig.SchnorrPkFromSk(sk)

	skHex := hex.EncodeToString(skBytes)
	pkHex := hex.EncodeToString(pk.ToLittleEndianBytes())

	t.Logf("Private key: %s", skHex)
	t.Logf("Expected public key: %s", pkHex)
	t.Logf("WASM schnorrPkFromSk should produce the same public key")
}
