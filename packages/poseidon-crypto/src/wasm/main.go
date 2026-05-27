//go:build js && wasm

package main

import (
	"encoding/hex"
	"fmt"
	"syscall/js"

	curve "github.com/elliottech/poseidon_crypto/curve/ecgfp5"
	g "github.com/elliottech/poseidon_crypto/field/goldilocks"
	p2 "github.com/elliottech/poseidon_crypto/hash/poseidon2_goldilocks_plonky2"
	sig "github.com/elliottech/poseidon_crypto/signature/schnorr"
)

// Helper function to convert JS Uint8Array to Go []byte
func uint8ArrayToBytes(value js.Value) []byte {
	length := value.Get("length").Int()
	bytes := make([]byte, length)
	js.CopyBytesToGo(bytes, value)
	return bytes
}

// Helper function to convert Go []byte to JS Uint8Array
func bytesToUint8Array(bytes []byte) js.Value {
	uint8Array := js.Global().Get("Uint8Array").New(len(bytes))
	js.CopyBytesToJS(uint8Array, bytes)
	return uint8Array
}

// Helper function to handle errors in JS functions
func handleError(err error) js.Value {
	obj := js.Global().Get("Object").New()
	obj.Set("error", err.Error())
	return obj
}

// Poseidon2 hash function - accepts hex string or Uint8Array, returns hex string
func poseidon2Hash(this js.Value, args []js.Value) interface{} {
	if len(args) < 1 {
		return handleError(fmt.Errorf("poseidon2Hash requires 1 argument"))
	}

	var inputBytes []byte
	if args[0].Type() == js.TypeString {
		// Decode hex string
		hexStr := args[0].String()
		var err error
		inputBytes, err = hex.DecodeString(hexStr)
		if err != nil {
			return handleError(fmt.Errorf("invalid hex string: %w", err))
		}
	} else {
		// Assume Uint8Array
		inputBytes = uint8ArrayToBytes(args[0])
	}

	// Convert bytes to Goldilocks field elements
	if len(inputBytes)%g.Bytes != 0 {
		return handleError(fmt.Errorf("input length must be multiple of %d bytes", g.Bytes))
	}

	numElements := len(inputBytes) / g.Bytes
	elements := make([]g.GoldilocksField, numElements)
	for i := 0; i < numElements; i++ {
		elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
	}

	// Hash
	hashOut := p2.HashNoPad(elements)
	hashBytes := hashOut.ToLittleEndianBytes()

	// Return as hex string
	return hex.EncodeToString(hashBytes)
}

// Poseidon2 hash to quintic extension - accepts hex string or Uint8Array, returns hex string
func poseidon2HashFp5(this js.Value, args []js.Value) interface{} {
	if len(args) < 1 {
		return handleError(fmt.Errorf("poseidon2HashFp5 requires 1 argument"))
	}

	var inputBytes []byte
	if args[0].Type() == js.TypeString {
		hexStr := args[0].String()
		var err error
		inputBytes, err = hex.DecodeString(hexStr)
		if err != nil {
			return handleError(fmt.Errorf("invalid hex string: %w", err))
		}
	} else {
		inputBytes = uint8ArrayToBytes(args[0])
	}

	if len(inputBytes)%g.Bytes != 0 {
		return handleError(fmt.Errorf("input length must be multiple of %d bytes", g.Bytes))
	}

	numElements := len(inputBytes) / g.Bytes
	elements := make([]g.GoldilocksField, numElements)
	for i := 0; i < numElements; i++ {
		elements[i] = g.FromCanonicalLittleEndianBytesF(inputBytes[i*g.Bytes : (i+1)*g.Bytes])
	}

	hashOut := p2.HashToQuinticExtension(elements)
	hashBytes := hashOut.ToLittleEndianBytes()

	return hex.EncodeToString(hashBytes)
}

// Poseidon2 hash two-to-one - accepts two hex strings, returns hex string
func poseidon2HashTwoToOne(this js.Value, args []js.Value) interface{} {
	if len(args) < 2 {
		return handleError(fmt.Errorf("poseidon2HashTwoToOne requires 2 arguments"))
	}

	// Parse first hash
	hash1Bytes, err := hex.DecodeString(args[0].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid hex string for hash1: %w", err))
	}
	hash1, err := p2.HashOutFromLittleEndianBytes(hash1Bytes)
	if err != nil {
		return handleError(err)
	}

	// Parse second hash
	hash2Bytes, err := hex.DecodeString(args[1].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid hex string for hash2: %w", err))
	}
	hash2, err := p2.HashOutFromLittleEndianBytes(hash2Bytes)
	if err != nil {
		return handleError(err)
	}

	// Hash two to one
	result := p2.HashTwoToOne(hash1, hash2)
	resultBytes := result.ToLittleEndianBytes()

	return hex.EncodeToString(resultBytes)
}

// Generate Schnorr key pair - returns {privateKey: hex, publicKey: hex}
func schnorrGenerateKeys(this js.Value, args []js.Value) interface{} {
	// Generate random scalar for private key
	sk := curve.SampleScalar()
	pk := sig.SchnorrPkFromSk(sk)

	// Convert to bytes
	skBytes := sk.ToLittleEndianBytes()
	pkBytes := pk.ToLittleEndianBytes()

	// Create result object
	result := js.Global().Get("Object").New()
	result.Set("privateKey", hex.EncodeToString(skBytes[:]))
	result.Set("publicKey", hex.EncodeToString(pkBytes))

	return result
}

// Derive public key from private key - accepts hex string, returns hex string
func schnorrPkFromSk(this js.Value, args []js.Value) interface{} {
	if len(args) < 1 {
		return handleError(fmt.Errorf("schnorrPkFromSk requires 1 argument"))
	}

	skBytes, err := hex.DecodeString(args[0].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid hex string: %w", err))
	}

	if len(skBytes) != 40 {
		return handleError(fmt.Errorf("private key must be 40 bytes, got %d", len(skBytes)))
	}

	sk := curve.ScalarElementFromLittleEndianBytes(skBytes)
	pk := sig.SchnorrPkFromSk(sk)
	pkBytes := pk.ToLittleEndianBytes()

	return hex.EncodeToString(pkBytes)
}

// Sign a message - accepts messageHex (hex string) and privateKeyHex (hex string), returns signature hex
func schnorrSign(this js.Value, args []js.Value) interface{} {
	if len(args) < 2 {
		return handleError(fmt.Errorf("schnorrSign requires 2 arguments: message (hex), privateKey (hex)"))
	}

	// Parse message (should be raw bytes to be hashed)
	msgBytes, err := hex.DecodeString(args[0].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid message hex: %w", err))
	}

	// Parse private key
	skBytes, err := hex.DecodeString(args[1].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid private key hex: %w", err))
	}

	if len(skBytes) != 40 {
		return handleError(fmt.Errorf("private key must be 40 bytes, got %d", len(skBytes)))
	}

	sk := curve.ScalarElementFromLittleEndianBytes(skBytes)

	// Hash the message to Fp5
	if len(msgBytes)%g.Bytes != 0 {
		return handleError(fmt.Errorf("message length must be multiple of %d bytes", g.Bytes))
	}

	numElements := len(msgBytes) / g.Bytes
	elements := make([]g.GoldilocksField, numElements)
	for i := 0; i < numElements; i++ {
		elements[i] = g.FromCanonicalLittleEndianBytesF(msgBytes[i*g.Bytes : (i+1)*g.Bytes])
	}

	hashedMsg := p2.HashToQuinticExtension(elements)

	// Sign
	signature := sig.SchnorrSignHashedMessage(hashedMsg, sk)
	sigBytes := signature.ToBytes()

	return hex.EncodeToString(sigBytes)
}

// Verify a signature - accepts messageHex, publicKeyHex, signatureHex, returns boolean
func schnorrVerify(this js.Value, args []js.Value) interface{} {
	if len(args) < 3 {
		return handleError(fmt.Errorf("schnorrVerify requires 3 arguments: message (hex), publicKey (hex), signature (hex)"))
	}

	// Parse message
	msgBytes, err := hex.DecodeString(args[0].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid message hex: %w", err))
	}

	// Parse public key
	pkBytes, err := hex.DecodeString(args[1].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid public key hex: %w", err))
	}

	// Parse signature
	sigBytes, err := hex.DecodeString(args[2].String())
	if err != nil {
		return handleError(fmt.Errorf("invalid signature hex: %w", err))
	}

	// Hash the message to Fp5
	if len(msgBytes)%g.Bytes != 0 {
		return handleError(fmt.Errorf("message length must be multiple of %d bytes", g.Bytes))
	}

	numElements := len(msgBytes) / g.Bytes
	elements := make([]g.GoldilocksField, numElements)
	for i := 0; i < numElements; i++ {
		elements[i] = g.FromCanonicalLittleEndianBytesF(msgBytes[i*g.Bytes : (i+1)*g.Bytes])
	}

	hashedMsg := p2.HashToQuinticExtension(elements)

	// Verify using the Validate function
	err = sig.Validate(pkBytes, hashedMsg.ToLittleEndianBytes(), sigBytes)
	if err != nil {
		return false
	}

	return true
}

func main() {
	c := make(chan struct{})

	// Register JavaScript functions
	js.Global().Set("poseidon2Hash", js.FuncOf(poseidon2Hash))
	js.Global().Set("poseidon2HashFp5", js.FuncOf(poseidon2HashFp5))
	js.Global().Set("poseidon2HashTwoToOne", js.FuncOf(poseidon2HashTwoToOne))
	js.Global().Set("schnorrGenerateKeys", js.FuncOf(schnorrGenerateKeys))
	js.Global().Set("schnorrPkFromSk", js.FuncOf(schnorrPkFromSk))
	js.Global().Set("schnorrSign", js.FuncOf(schnorrSign))
	js.Global().Set("schnorrVerify", js.FuncOf(schnorrVerify))

	fmt.Println("Poseidon Crypto WASM module loaded")
	fmt.Println("Available functions:")
	fmt.Println("  - poseidon2Hash(input: hex string) -> hex string")
	fmt.Println("  - poseidon2HashFp5(input: hex string) -> hex string")
	fmt.Println("  - poseidon2HashTwoToOne(hash1: hex, hash2: hex) -> hex string")
	fmt.Println("  - schnorrGenerateKeys() -> {privateKey: hex, publicKey: hex}")
	fmt.Println("  - schnorrPkFromSk(privateKey: hex) -> hex string")
	fmt.Println("  - schnorrSign(message: hex, privateKey: hex) -> hex string")
	fmt.Println("  - schnorrVerify(message: hex, publicKey: hex, signature: hex) -> boolean")

	<-c
}
