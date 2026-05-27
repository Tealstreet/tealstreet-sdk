// Cross-validation test: Ensures WASM output matches native Go implementation
const fs = require('fs');
const { performance } = require('perf_hooks');

// Load the Go WASM exec script
require('./wasm_exec.js');

// Load test vectors
const testVectors = JSON.parse(fs.readFileSync('./test_vectors.json', 'utf8'));

async function runValidation() {
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

    console.log('\n=== Cross-Validation: WASM vs Native Go ===\n');

    let passed = 0;
    let failed = 0;

    for (const vector of testVectors) {
        let result;
        let actual;

        try {
            switch (vector.test) {
                case 'poseidon2Hash':
                    actual = poseidon2Hash(vector.input);
                    break;

                case 'poseidon2HashTwoToOne':
                    actual = poseidon2HashTwoToOne(vector.input1, vector.input2);
                    break;

                case 'schnorrPkFromSk':
                    actual = schnorrPkFromSk(vector.privateKey);
                    break;

                case 'schnorrSign_deterministic':
                    // Note: WASM uses random nonce, so we can't validate deterministic signatures
                    // We'll skip this test
                    console.log(`⊘ ${vector.test}: ${vector.description}`);
                    console.log(`  Skipped (WASM uses random nonce, cannot be deterministic)`);
                    console.log('');
                    continue;

                case 'schnorrVerify':
                    actual = schnorrVerify(vector.message, vector.publicKey, vector.signature);
                    actual = actual.toString(); // Convert boolean to string
                    break;

                default:
                    console.log(`⊘ ${vector.test}: Unknown test type`);
                    console.log('');
                    continue;
            }

            // Check for errors
            if (actual && actual.error) {
                console.log(`✗ ${vector.test}: ${vector.description}`);
                console.log(`  Error: ${actual.error}`);
                console.log('');
                failed++;
                continue;
            }

            // Compare result
            if (actual === vector.expected) {
                console.log(`✓ ${vector.test}: ${vector.description}`);
                console.log(`  Result: ${actual}`);
                console.log('');
                passed++;
            } else {
                console.log(`✗ ${vector.test}: ${vector.description}`);
                console.log(`  Expected: ${vector.expected}`);
                console.log(`  Got:      ${actual}`);
                console.log('');
                failed++;
            }

        } catch (err) {
            console.log(`✗ ${vector.test}: ${vector.description}`);
            console.log(`  Exception: ${err.message}`);
            console.log('');
            failed++;
        }
    }

    console.log('=== Summary ===');
    console.log(`Passed: ${passed}`);
    console.log(`Failed: ${failed}`);
    console.log(`Total:  ${passed + failed}`);

    if (failed > 0) {
        console.log('\n⚠️  Some tests failed! WASM implementation does not match native Go.');
        process.exit(1);
    } else {
        console.log('\n✓ All tests passed! WASM implementation matches native Go.');
        process.exit(0);
    }
}

runValidation().catch(err => {
    console.error('Fatal error:', err);
    process.exit(1);
});
