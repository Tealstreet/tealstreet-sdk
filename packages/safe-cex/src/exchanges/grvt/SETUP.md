# GRVT Exchange Setup Guide

## Quick Start

### 1. Get Your Signer Address

Run the helper script to find your Ethereum signer address:

```bash
cd packages/safe-cex
npx tsx src/exchanges/grvt/get-signer-address.ts
```

This will output something like:

```
Signer Address: 0x8B7537a4f6D63C861dcbB7559C724fc17Fe6De51
```

**IMPORTANT:** Copy this address - you'll need it in the next step.

### 2. Tag the Signer Address in GRVT UI

1. Go to the GRVT website:
   - **Mainnet**: https://grvt.io/
   - **Testnet**: https://testnet.grvt.io/

2. Navigate to **Account** → **API Keys**

3. Find or create an API key

4. **Tag the Ethereum address** (from step 1) to your API key
   - Look for an option like "Add Signer" or "Tag Address"
   - Paste your Ethereum address (e.g., `0x8B7537a4f6D63C861dcbB7559C724fc17Fe6De51`)
   - Save the changes

5. Ensure the API key has **trading permissions** enabled

### 3. Verify Configuration

Your `.apikey.json` file should look like this:

```json
{
  "apiKey": "YOUR_API_KEY",
  "secret": "0xYOUR_ETHEREUM_PRIVATE_KEY",
  "testnet": false,
  "accountId": "YOUR_ACCOUNT_ID",
  "subAccountId": "YOUR_SUB_ACCOUNT_ID"
}
```

Fields:
- `apiKey`: Your GRVT API key (from the GRVT UI)
- `secret`: Your Ethereum private key (64-char hex, with or without `0x` prefix)
- `testnet`: `true` for testnet, `false` for mainnet
- `accountId`: Your main account ID (optional)
- `subAccountId`: Your sub-account ID (optional, defaults to main account)

### 4. Test Order Placement

Run the integration test to verify everything works:

```bash
npx vitest run src/exchanges/grvt/__tests__/grvt.order-flow.test.ts
```

If you see a **403 Forbidden** error with code **1001**, it means the signer address hasn't been tagged correctly. Go back to step 2.

## Common Issues

### 403 Forbidden - Error Code 1001

**Error**: "You are not authorized to access this functionality"

**Cause**: The signer's Ethereum address is not tagged to the API key in the GRVT UI.

**Solution**: Follow step 2 above to tag the address.

### 403 Forbidden - Error Code 1400

**Error**: "Signer does not have trade permission"

**Cause**: The API key or signer doesn't have trading permissions enabled.

**Solution**: In the GRVT UI, enable trading permissions for your API key.

### 403 Forbidden - Error Code 2000

**Error**: "Signature is from an unauthorized signer"

**Cause**: The signer address in the request doesn't match the tagged address.

**Solution**:
1. Run `npx tsx src/exchanges/grvt/get-signer-address.ts` to get your current signer address
2. Verify it matches the address tagged in the GRVT UI
3. If different, update the tagged address in the GRVT UI

### Wrong Private Key

If you're using the wrong private key, the derived Ethereum address won't match what you tagged in the GRVT UI.

**Solution**:
1. Make sure the private key in `.apikey.json` matches the one you used to derive the tagged address
2. Or, derive the address from your current private key and update it in the GRVT UI

## Testing Without Real Funds

If you want to test without risking real funds:

1. Use **GRVT Testnet** (set `"testnet": true` in `.apikey.json`)
2. Create a testnet API key at https://testnet.grvt.io/
3. Get testnet funds from a faucet (if available)
4. Tag your signer address to the testnet API key
5. Run tests against testnet

## Need Help?

- **GRVT Documentation**: https://api-docs.grvt.io/
- **Local Documentation**: See `CLAUDE.md` in this directory
- **Helper Script**: `npx tsx src/exchanges/grvt/get-signer-address.ts`
- **Debug Test**: `npx vitest run src/exchanges/grvt/__tests__/grvt.debug.test.ts`

## Architecture Overview

GRVT uses EIP-712 signing for order placement:

1. **Private Key** → Derives **Ethereum Address** (signer address)
2. **Signer Address** → Must be **tagged to API key** in GRVT UI
3. **API Key + Cookie** → Authenticates session
4. **Orders** → Signed with EIP-712 using private key
5. **GRVT API** → Verifies signature matches tagged signer address

Without step 2 (tagging), the API rejects orders with a 403 error.
