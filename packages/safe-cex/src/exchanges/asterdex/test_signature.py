#!/usr/bin/env python3
"""
AsterDEX signature generation test script
Based on the official documentation from:
https://github.com/asterdex/api-docs/blob/master/aster-finance-futures-api-v3.md
"""

import json
import time
from eth_abi import encode
from eth_account import Account
from eth_account.messages import encode_defunct
from web3 import Web3

# Test parameters - using fixed values for reproducibility
USER_ADDRESS = "0x1234567890123456789012345678901234567890"
SIGNER_ADDRESS = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
PRIVATE_KEY = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
NONCE = 1234567890000000  # microseconds
TIMESTAMP = 1234567890000  # milliseconds

def trim_dict(d):
    """Convert all dictionary values to strings and handle nested structures"""
    result = {}
    for key in d:
        value = d[key]
        if value is None:
            continue
        if isinstance(value, list):
            new_value = []
            for item in value:
                if isinstance(item, dict):
                    new_value.append(json.dumps(trim_dict(item)))
                else:
                    new_value.append(str(item))
            result[key] = json.dumps(new_value)
        elif isinstance(value, dict):
            result[key] = json.dumps(trim_dict(value))
        else:
            result[key] = str(value)
    return result

def generate_signature(request_params, user_address, signer_address, nonce, private_key):
    """Generate signature following AsterDEX v3 API documentation"""

    # 1. Add recvWindow and timestamp to params
    params_with_timestamp = {
        **request_params,
        'recvWindow': 50000,
        'timestamp': TIMESTAMP
    }

    # 2. Trim the parameters (convert to strings)
    trimmed_params = trim_dict(params_with_timestamp)

    # 3. Create sorted JSON string (no spaces)
    json_str = json.dumps(trimmed_params, sort_keys=True, separators=(',', ':')).replace("'", '"')

    print(f"JSON string: {json_str}")

    # 4. Encode using ABI: [string, address, address, uint256]
    encoded = encode(
        ['string', 'address', 'address', 'uint256'],
        [json_str, user_address, signer_address, nonce]
    )

    print(f"Encoded hex: 0x{encoded.hex()}")

    # 5. Hash with Keccak256
    keccak_hash = Web3.keccak(encoded)
    print(f"Keccak256 hash: {keccak_hash.hex()}")

    # 6. Sign the hash directly (treat it as a message hash)
    from eth_account.messages import _hash_eip191_message
    from eth_keys import keys

    # Create a private key object
    priv_key = keys.PrivateKey(bytes.fromhex(private_key.replace('0x', '')))

    # Sign the hash directly (no additional hashing)
    signature_obj = priv_key.sign_msg_hash(keccak_hash)

    # 7. Format signature as hex (v, r, s order)
    signature = '0x' + signature_obj.to_hex()
    print(f"Signature: {signature}")

    return signature

def test_simple_params():
    """Test with simple parameters"""
    print("\n=== Test Case 1: Simple Parameters ===")
    params = {
        'symbol': 'BTCUSDT',
        'side': 'BUY'
    }

    signature = generate_signature(
        params,
        USER_ADDRESS,
        SIGNER_ADDRESS,
        NONCE,
        PRIVATE_KEY
    )

    return signature

def test_complex_params():
    """Test with complex parameters including nested objects and arrays"""
    print("\n=== Test Case 2: Complex Parameters ===")
    params = {
        'symbol': 'BTCUSDT',
        'side': 'BUY',
        'type': 'LIMIT',
        'quantity': 0.001,
        'price': 40000,
        'nested': {
            'value': 123,
            'flag': True
        },
        'array': [1, 2, 'test']
    }

    signature = generate_signature(
        params,
        USER_ADDRESS,
        SIGNER_ADDRESS,
        NONCE,
        PRIVATE_KEY
    )

    return signature

def test_empty_params():
    """Test with empty parameters"""
    print("\n=== Test Case 3: Empty Parameters ===")
    params = {}

    signature = generate_signature(
        params,
        USER_ADDRESS,
        SIGNER_ADDRESS,
        NONCE,
        PRIVATE_KEY
    )

    return signature

def test_order_params():
    """Test typical order parameters"""
    print("\n=== Test Case 4: Order Parameters ===")
    params = {
        'symbol': 'ETHUSDT',
        'side': 'SELL',
        'type': 'LIMIT',
        'quantity': '0.5',
        'price': '2500.50',
        'timeInForce': 'GTC'
    }

    signature = generate_signature(
        params,
        USER_ADDRESS,
        SIGNER_ADDRESS,
        NONCE,
        PRIVATE_KEY
    )

    return signature

if __name__ == "__main__":
    print("AsterDEX Python Signature Generation Test")
    print("==========================================")
    print(f"User Address: {USER_ADDRESS}")
    print(f"Signer Address: {SIGNER_ADDRESS}")
    print(f"Nonce: {NONCE}")
    print(f"Timestamp: {TIMESTAMP}")
    print(f"Private Key: {PRIVATE_KEY[:10]}...{PRIVATE_KEY[-4:]}")

    # Run all test cases
    sig1 = test_simple_params()
    sig2 = test_complex_params()
    sig3 = test_empty_params()
    sig4 = test_order_params()

    print("\n=== Summary of Generated Signatures ===")
    print(f"Test 1 (Simple): {sig1}")
    print(f"Test 2 (Complex): {sig2}")
    print(f"Test 3 (Empty): {sig3}")
    print(f"Test 4 (Order): {sig4}")