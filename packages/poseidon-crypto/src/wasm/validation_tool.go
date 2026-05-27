//go:build !js

package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"

	curve "github.com/elliottech/poseidon_crypto/curve/ecgfp5"
	g "github.com/elliottech/poseidon_crypto/field/goldilocks"
	p2 "github.com/elliottech/poseidon_crypto/hash/poseidon2_goldilocks_plonky2"
	sig "github.com/elliottech/poseidon_crypto/signature/schnorr"
)

type TestVector struct {
	Test        string `json:"test"`
	Description string `json:"description"`
	Input       string `json:"input,omitempty"`
	Input1      string `json:"input1,omitempty"`
	Input2      string `json:"input2,omitempty"`
	PrivateKey  string `json:"privateKey,omitempty"`
	PublicKey   string `json:"publicKey,omitempty"`
	Message     string `json:"message,omitempty"`
	Signature   string `json:"signature,omitempty"`
	Expected    string `json:"expected"`
}

func main() {
	vectors := []TestVector{}

	// Test 1: Poseidon2 Hash - zero input
	{
		input := "0000000000000000000000000000000000000000000000000000000000000000"
		inputBytes, _ := hex.DecodeString(input)
		numElements := len(inputBytes) / g.Bytes
		elements := make([]g.GoldilocksField, numElements)
		for i := 0; i < numElements; i++ {
			elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
		}
		hashOut := p2.HashNoPad(elements)
		expected := hex.EncodeToString(hashOut.ToLittleEndianBytes())

		vectors = append(vectors, TestVector{
			Test:        "poseidon2Hash",
			Description: "Zero input (4 elements / 32 bytes)",
			Input:       input,
			Expected:    expected,
		})
	}

	// Test 2: Poseidon2 Hash - single element
	{
		input := "0100000000000000"
		inputBytes, _ := hex.DecodeString(input)
		numElements := len(inputBytes) / g.Bytes
		elements := make([]g.GoldilocksField, numElements)
		for i := 0; i < numElements; i++ {
			elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
		}
		hashOut := p2.HashNoPad(elements)
		expected := hex.EncodeToString(hashOut.ToLittleEndianBytes())

		vectors = append(vectors, TestVector{
			Test:        "poseidon2Hash",
			Description: "Single element (value: 1)",
			Input:       input,
			Expected:    expected,
		})
	}

	// Test 3: Poseidon2 Hash - pattern
	{
		input := "0123456789abcdef0011223344556677"
		inputBytes, _ := hex.DecodeString(input)
		numElements := len(inputBytes) / g.Bytes
		elements := make([]g.GoldilocksField, numElements)
		for i := 0; i < numElements; i++ {
			elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
		}
		hashOut := p2.HashNoPad(elements)
		expected := hex.EncodeToString(hashOut.ToLittleEndianBytes())

		vectors = append(vectors, TestVector{
			Test:        "poseidon2Hash",
			Description: "Pattern input (2 elements)",
			Input:       input,
			Expected:    expected,
		})
	}

	// Test 4: Hash Two-to-One
	{
		hash1Str := "1cc3145a75f1ab638273d45f3c163881281438ebb436917a2d451041d0f18795"
		hash2Str := "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"

		hash1Bytes, _ := hex.DecodeString(hash1Str)
		hash2Bytes, _ := hex.DecodeString(hash2Str)

		hash1, _ := p2.HashOutFromLittleEndianBytes(hash1Bytes)
		hash2, _ := p2.HashOutFromLittleEndianBytes(hash2Bytes)

		result := p2.HashTwoToOne(hash1, hash2)
		expected := hex.EncodeToString(result.ToLittleEndianBytes())

		vectors = append(vectors, TestVector{
			Test:        "poseidon2HashTwoToOne",
			Description: "Combine two hashes",
			Input1:      hash1Str,
			Input2:      hash2Str,
			Expected:    expected,
		})
	}

	// Test 5: Public Key Derivation (deterministic)
	{
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

		vectors = append(vectors, TestVector{
			Test:        "schnorrPkFromSk",
			Description: "Derive public key from private key",
			PrivateKey:  skHex,
			Expected:    pkHex,
		})
	}

	// Test 6: Schnorr Signature (deterministic nonce - using internal function)
	{
		skBytes := []byte{
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		}
		kBytes := []byte{
			0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		}

		sk := curve.ScalarElementFromLittleEndianBytes(skBytes)
		k := curve.ScalarElementFromLittleEndianBytes(kBytes)
		pk := sig.SchnorrPkFromSk(sk)

		msgStr := "0000000000000000000000000000000000000000000000000000000000000001"
		msgBytes, _ := hex.DecodeString(msgStr)

		numElements := len(msgBytes) / g.Bytes
		elements := make([]g.GoldilocksField, numElements)
		for i := 0; i < numElements; i++ {
			elements[i] = g.FromCanonicalLittleEndianBytesF(msgBytes[i*g.Bytes : (i+1)*g.Bytes])
		}
		hashedMsg := p2.HashToQuinticExtension(elements)

		signature := sig.SchnorrSignHashedMessage2(hashedMsg, sk, k)
		sigHex := hex.EncodeToString(signature.ToBytes())
		pkHex := hex.EncodeToString(pk.ToLittleEndianBytes())
		skHex := hex.EncodeToString(skBytes)

		// Verify it works
		isValid := sig.IsSchnorrSignatureValid(&pk, &hashedMsg, signature)

		vectors = append(vectors, TestVector{
			Test:        "schnorrSign_deterministic",
			Description: fmt.Sprintf("Deterministic signature (valid: %v)", isValid),
			Message:     msgStr,
			PrivateKey:  skHex,
			PublicKey:   pkHex,
			Expected:    sigHex,
		})
	}

	// Test 7: Schnorr Verify
	{
		skBytes := []byte{
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		}
		kBytes := []byte{
			0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
			0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		}

		sk := curve.ScalarElementFromLittleEndianBytes(skBytes)
		k := curve.ScalarElementFromLittleEndianBytes(kBytes)
		pk := sig.SchnorrPkFromSk(sk)

		msgStr := "0000000000000000000000000000000000000000000000000000000000000001"
		msgBytes, _ := hex.DecodeString(msgStr)

		numElements := len(msgBytes) / g.Bytes
		elements := make([]g.GoldilocksField, numElements)
		for i := 0; i < numElements; i++ {
			elements[i] = g.FromCanonicalLittleEndianBytesF(msgBytes[i*g.Bytes : (i+1)*g.Bytes])
		}
		hashedMsg := p2.HashToQuinticExtension(elements)

		signature := sig.SchnorrSignHashedMessage2(hashedMsg, sk, k)
		sigHex := hex.EncodeToString(signature.ToBytes())
		pkHex := hex.EncodeToString(pk.ToLittleEndianBytes())

		vectors = append(vectors, TestVector{
			Test:        "schnorrVerify",
			Description: "Verify valid signature (should return true)",
			Message:     msgStr,
			PublicKey:   pkHex,
			Signature:   sigHex,
			Expected:    "true",
		})
	}

	// Output JSON
	jsonBytes, err := json.MarshalIndent(vectors, "", "  ")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error marshaling JSON: %v\n", err)
		os.Exit(1)
	}

	fmt.Println(string(jsonBytes))
}
