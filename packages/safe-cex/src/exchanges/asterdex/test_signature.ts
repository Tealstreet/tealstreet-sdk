#!/usr/bin/env node
/**
 * AsterDEX signature generation test script
 * This should produce identical results to the Python version
 */

import { ethers } from 'ethers';

// Test parameters - using fixed values for reproducibility (same as Python)
const USER_ADDRESS = '0x1234567890123456789012345678901234567890';
const SIGNER_ADDRESS = '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd';
const PRIVATE_KEY = '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
const NONCE = 1234567890000000; // microseconds
const TIMESTAMP = 1234567890000; // milliseconds

/**
 * Convert all values in an object to strings and handle nested structures
 */
function trimDict(obj: any): any {
  const result: any = {};

  for (const key in obj) {
    const value = obj[key];

    if (value === null || value === undefined) {
      continue;
    }

    if (Array.isArray(value)) {
      // Convert array items to strings and stringify the array
      const stringArray = value.map(item => {
        if (typeof item === 'object' && item !== null) {
          return JSON.stringify(trimDict(item));
        }
        return String(item);
      });
      result[key] = JSON.stringify(stringArray);
    } else if (typeof value === 'object') {
      // Recursively handle nested objects
      result[key] = JSON.stringify(trimDict(value));
    } else {
      // Convert primitives to strings
      result[key] = String(value);
    }
  }

  return result;
}

/**
 * Generate signature following AsterDEX v3 API documentation
 */
function generateSignature(
  requestParams: Record<string, any>,
  userAddress: string,
  signerAddress: string,
  nonce: number,
  privateKey: string
): string {
  // 1. Add recvWindow and timestamp to params
  const paramsWithTimestamp = {
    ...requestParams,
    recvWindow: 5000,
    timestamp: TIMESTAMP,
  };

  // 2. Trim the parameters (convert to strings)
  const trimmedParams = trimDict(paramsWithTimestamp);

  // 3. Create sorted JSON string (no spaces, single quotes replaced with double)
  const jsonStr = JSON.stringify(trimmedParams, Object.keys(trimmedParams).sort())
    .replace(/ /g, '')
    .replace(/'/g, '"');

  console.log(`JSON string: ${jsonStr}`);

  // 4. Encode using ABI: [string, address, address, uint256]
  const encoded = ethers.AbiCoder.defaultAbiCoder().encode(
    ['string', 'address', 'address', 'uint256'],
    [jsonStr, userAddress, signerAddress, BigInt(nonce)],
  );

  console.log(`Encoded hex: ${encoded}`);

  // 5. Hash the encoded data with Keccak256
  const keccakHash = ethers.keccak256(encoded);
  console.log(`Keccak256 hash: ${keccakHash}`);

  // 6. Create signing key from private key
  const signingKey = new ethers.SigningKey(privateKey);

  // 7. Sign the hash
  const signature = signingKey.sign(keccakHash);

  // 8. Format signature
  const formattedSignature = ethers.Signature.from(signature).serialized;
  console.log(`Signature: ${formattedSignature}`);

  return formattedSignature;
}

function testSimpleParams(): string {
  console.log('\n=== Test Case 1: Simple Parameters ===');
  const params = {
    symbol: 'BTCUSDT',
    side: 'BUY',
  };

  return generateSignature(
    params,
    USER_ADDRESS,
    SIGNER_ADDRESS,
    NONCE,
    PRIVATE_KEY
  );
}

function testComplexParams(): string {
  console.log('\n=== Test Case 2: Complex Parameters ===');
  const params = {
    symbol: 'BTCUSDT',
    side: 'BUY',
    type: 'LIMIT',
    quantity: 0.001,
    price: 40000,
    nested: {
      value: 123,
      flag: true,
    },
    array: [1, 2, 'test'],
  };

  return generateSignature(
    params,
    USER_ADDRESS,
    SIGNER_ADDRESS,
    NONCE,
    PRIVATE_KEY
  );
}

function testEmptyParams(): string {
  console.log('\n=== Test Case 3: Empty Parameters ===');
  const params = {};

  return generateSignature(
    params,
    USER_ADDRESS,
    SIGNER_ADDRESS,
    NONCE,
    PRIVATE_KEY
  );
}

function testOrderParams(): string {
  console.log('\n=== Test Case 4: Order Parameters ===');
  const params = {
    symbol: 'ETHUSDT',
    side: 'SELL',
    type: 'LIMIT',
    quantity: '0.5',
    price: '2500.50',
    timeInForce: 'GTC',
  };

  return generateSignature(
    params,
    USER_ADDRESS,
    SIGNER_ADDRESS,
    NONCE,
    PRIVATE_KEY
  );
}

// Main execution
console.log('AsterDEX TypeScript Signature Generation Test');
console.log('==============================================');
console.log(`User Address: ${USER_ADDRESS}`);
console.log(`Signer Address: ${SIGNER_ADDRESS}`);
console.log(`Nonce: ${NONCE}`);
console.log(`Timestamp: ${TIMESTAMP}`);
console.log(`Private Key: ${PRIVATE_KEY.slice(0, 10)}...${PRIVATE_KEY.slice(-4)}`);

// Run all test cases
const sig1 = testSimpleParams();
const sig2 = testComplexParams();
const sig3 = testEmptyParams();
const sig4 = testOrderParams();

console.log('\n=== Summary of Generated Signatures ===');
console.log(`Test 1 (Simple): ${sig1}`);
console.log(`Test 2 (Complex): ${sig2}`);
console.log(`Test 3 (Empty): ${sig3}`);
console.log(`Test 4 (Order): ${sig4}`);