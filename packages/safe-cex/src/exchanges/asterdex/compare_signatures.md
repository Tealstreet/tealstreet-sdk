# AsterDEX Signature Comparison: Python vs TypeScript

## Test Case 1: Simple Parameters
- **Input**: `{"symbol": "BTCUSDT", "side": "BUY"}`
- **JSON String**: `{"recvWindow":"50000","side":"BUY","symbol":"BTCUSDT","timestamp":"1234567890000"}`
- **Keccak256 Hash**:
  - Python: `8ae6051651cdd966797478f657aff4b01189276f9c8a41146155da0f94a7e76f`
  - TypeScript: `0x8ae6051651cdd966797478f657aff4b01189276f9c8a41146155da0f94a7e76f`
  - ✅ **MATCH** (same hash)
- **Signature**:
  - Python: `0x6bce8df2dcdcd2a06b68e9e7568f5bd151c9aed9fcb2a1599195c83b581146af68302ebec6b311b2c8b3a946795bb4a9355358769e5ee511c1bcc8b469abfb0f01`
  - TypeScript: `0x6bce8df2dcdcd2a06b68e9e7568f5bd151c9aed9fcb2a1599195c83b581146af68302ebec6b311b2c8b3a946795bb4a9355358769e5ee511c1bcc8b469abfb0f1c`
  - ❌ **DIFFERENCE**: Last byte differs (Python: `01`, TypeScript: `1c`)

## Test Case 2: Complex Parameters
- **Keccak256 Hash**:
  - Python: `53937742daca139a50955389a04d88b8d1a51d560b7e5f6a7bd55da5113ba237`
  - TypeScript: `0x12455fb3a8b7b236124703d5ec86b2e3ccdb7cb2bf973c7fb9abdf63f7c3ae60`
  - ❌ **DIFFERENT HASHES** - This indicates the JSON strings are different

## Test Case 3: Empty Parameters
- **Keccak256 Hash**:
  - Python: `b148a16a5ebc4684a9ecb1adf8f67414ca41d489151e35f3db3d19b138cf70fa`
  - TypeScript: `0xb148a16a5ebc4684a9ecb1adf8f67414ca41d489151e35f3db3d19b138cf70fa`
  - ✅ **MATCH** (same hash)
- **Signature**:
  - Python: `0x15b16105558a9333bd239a3af09d3abdc57e77da7fbf8b393e5eabd216b640e02a6da47ddf831d38c2371c6c1ed8997b4a291eb2d1b46d61fefdebc61f7e475101`
  - TypeScript: `0x15b16105558a9333bd239a3af09d3abdc57e77da7fbf8b393e5eabd216b640e02a6da47ddf831d38c2371c6c1ed8997b4a291eb2d1b46d61fefdebc61f7e47511c`
  - ❌ **DIFFERENCE**: Last byte differs (Python: `01`, TypeScript: `1c`)

## Test Case 4: Order Parameters
- **Keccak256 Hash**:
  - Python: `6d9df1f55268ea980eaf78216a1376879a22242798d7f3ed806e35b840f500d7`
  - TypeScript: `0x6d9df1f55268ea980eaf78216a1376879a22242798d7f3ed806e35b840f500d7`
  - ✅ **MATCH** (same hash)
- **Signature**:
  - Python: `0x1aca26ae2dddfdfc1faf0001843872661a351ea8226a4802e204215bdb3269a3572053a77e91d8d7bb5ce91f8caec0946a1f6bda12092440ec981785d3f98ba000`
  - TypeScript: `0x1aca26ae2dddfdfc1faf0001843872661a351ea8226a4802e204215bdb3269a3572053a77e91d8d7bb5ce91f8caec0946a1f6bda12092440ec981785d3f98ba01b`
  - ❌ **DIFFERENCE**: Last byte differs (Python: `00`, TypeScript: `1b`)

## Analysis

### Issue 1: Recovery ID (v value)
The last byte of the signature is the recovery ID (v value). The differences suggest:
- Python is producing v values of `00` or `01` (legacy format)
- TypeScript is producing v values of `1b` or `1c` (EIP-155 format, 27 or 28)

This is a known difference between different Ethereum libraries. The signature itself (r and s values) are identical, only the recovery ID format differs.

### Issue 2: Complex Object Serialization
Test Case 2 produces completely different hashes, indicating the JSON string serialization differs:
- Python: `"array":"[\"1\", \"2\", \"test\"]","nested":"{\"value\": \"123\", \"flag\": \"True\"}"`
- TypeScript: `"array":"[\"1\",\"2\",\"test\"]","nested":"{\"value\":\"123\",\"flag\":\"true\"}"`

Differences:
1. Python adds spaces after commas in arrays
2. Python capitalizes boolean `True` vs TypeScript `true`
3. Python adds spaces after colons in nested objects

### Solution
The signatures are functionally equivalent - the r and s values match perfectly. The only difference is the recovery ID format, which both formats are valid in Ethereum.

For the complex object serialization, we need to ensure consistent formatting between Python and TypeScript.