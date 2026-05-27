// Node.js test script for the WASM module
const fs = require('fs');
const { performance } = require('perf_hooks');

// Load the Go WASM exec script
require('./wasm_exec.js');

async function runTests() {
    console.log('Loading WASM module...');

    // Read the WASM file
    const wasmBuffer = fs.readFileSync('./poseidon_crypto.wasm');

    // Initialize Go
    const go = new Go();

    // Instantiate the WASM module
    const result = await WebAssembly.instantiate(wasmBuffer, go.importObject);

    // Run the Go program (non-blocking)
    go.run(result.instance);

    // Wait a bit for initialization
    await new Promise(resolve => setTimeout(resolve, 100));

    console.log('\n=== Starting Tests ===\n');

    // Test 1: Poseidon2 Hash
    console.log('Test 1: Poseidon2 Hash');
    try {
        const input = "0000000000000000000000000000000000000000000000000000000000000000";
        console.log('  Input:', input);
        const hash = poseidon2Hash(input);
        if (hash.error) {
            console.log('  ❌ Error:', hash.error);
        } else {
            console.log('  ✓ Hash:', hash);
        }
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    // Test 2: Hash Two-to-One
    console.log('\nTest 2: Poseidon2 Hash Two-to-One');
    try {
        const hash1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        const hash2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        console.log('  Hash1:', hash1);
        console.log('  Hash2:', hash2);
        const combined = poseidon2HashTwoToOne(hash1, hash2);
        if (combined.error) {
            console.log('  ❌ Error:', combined.error);
        } else {
            console.log('  ✓ Combined:', combined);
        }
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    // Test 3: Key Generation
    console.log('\nTest 3: Schnorr Key Generation');
    let keys;
    try {
        keys = schnorrGenerateKeys();
        if (keys.error) {
            console.log('  ❌ Error:', keys.error);
        } else {
            console.log('  ✓ Private Key:', keys.privateKey);
            console.log('  ✓ Public Key:', keys.publicKey);
        }
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    // Test 4: Derive Public Key from Private Key
    console.log('\nTest 4: Derive Public Key from Private Key');
    try {
        const derivedPk = schnorrPkFromSk(keys.privateKey);
        if (derivedPk.error) {
            console.log('  ❌ Error:', derivedPk.error);
        } else {
            const matches = derivedPk === keys.publicKey;
            console.log('  ✓ Derived PK:', derivedPk);
            console.log('  ✓ Matches generated PK:', matches);
        }
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    // Test 5: Sign and Verify
    console.log('\nTest 5: Schnorr Sign and Verify');
    try {
        const message = "0000000000000000000000000000000000000000000000000000000000000001";
        console.log('  Message:', message);

        // Sign
        const signature = schnorrSign(message, keys.privateKey);
        if (signature.error) {
            console.log('  ❌ Sign Error:', signature.error);
        } else {
            console.log('  ✓ Signature:', signature);

            // Verify with correct key
            const valid = schnorrVerify(message, keys.publicKey, signature);
            if (valid.error) {
                console.log('  ❌ Verify Error:', valid.error);
            } else {
                console.log('  ✓ Signature valid:', valid === true);
            }

            // Verify with wrong key (should fail)
            const wrongKeys = schnorrGenerateKeys();
            const invalidVerify = schnorrVerify(message, wrongKeys.publicKey, signature);
            console.log('  ✓ Wrong key verification (should be false):', invalidVerify === false);
        }
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    // Test 6: Performance test
    console.log('\nTest 6: Performance Test');
    try {
        const iterations = 100;

        // Hash performance
        const hashStart = performance.now();
        for (let i = 0; i < iterations; i++) {
            poseidon2Hash("0000000000000000000000000000000000000000000000000000000000000000");
        }
        const hashEnd = performance.now();
        console.log(`  ✓ Hashing: ${iterations} iterations in ${(hashEnd - hashStart).toFixed(2)}ms`);
        console.log(`    Average: ${((hashEnd - hashStart) / iterations).toFixed(2)}ms per hash`);

        // Sign performance
        const signStart = performance.now();
        for (let i = 0; i < iterations; i++) {
            schnorrSign("0000000000000000000000000000000000000000000000000000000000000001", keys.privateKey);
        }
        const signEnd = performance.now();
        console.log(`  ✓ Signing: ${iterations} iterations in ${(signEnd - signStart).toFixed(2)}ms`);
        console.log(`    Average: ${((signEnd - signStart) / iterations).toFixed(2)}ms per signature`);
    } catch (err) {
        console.log('  ❌ Exception:', err.message);
    }

    console.log('\n=== Tests Complete ===\n');
    process.exit(0);
}

runTests().catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
});
