#!/usr/bin/env tsx

/**
 * Helper script to get the signer address from your private key
 *
 * This address MUST be tagged to your API key in the GRVT UI:
 * - Testnet: https://testnet.grvt.io/exchange/account/api-keys
 * - Mainnet: https://grvt.io/exchange/account/api-keys
 */

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { ethers } from 'ethers';

const credPath = resolve(__dirname, '.apikey.json');

if (!existsSync(credPath)) {
  console.error('❌ Error: .apikey.json file not found');
  console.error(`Expected path: ${credPath}`);
  console.error('\nPlease create .apikey.json with your GRVT credentials');
  process.exit(1);
}

try {
  const configData = readFileSync(credPath, 'utf-8');
  const config = JSON.parse(configData);

  if (!config.secret) {
    console.error('❌ Error: No "secret" field found in .apikey.json');
    console.error('The secret field should contain your private key (0x-prefixed hex string)');
    process.exit(1);
  }

  // Create wallet from private key
  const wallet = new ethers.Wallet(config.secret);

  console.log('\n======================================');
  console.log('GRVT Signer Address');
  console.log('======================================\n');
  console.log('API Key:', config.apiKey?.substring(0, 15) + '...');
  console.log('Signer Address:', wallet.address);
  console.log('\n======================================');
  console.log('IMPORTANT: Tag this address to your API key');
  console.log('======================================\n');

  if (config.testnet) {
    console.log('📍 Go to: https://testnet.grvt.io/exchange/account/api-keys');
  } else {
    console.log('📍 Go to: https://grvt.io/exchange/account/api-keys');
  }

  console.log('\nSteps:');
  console.log('1. Navigate to the API Keys page (link above)');
  console.log('2. Find your API key:', config.apiKey?.substring(0, 15) + '...');
  console.log('3. Click "Edit" or "Manage"');
  console.log('4. Tag the Signer Address:', wallet.address);
  console.log('5. Save the changes');
  console.log('\n✅ After tagging the address, your API will work!\n');

  // Verify if the provided grvtSignerAddress matches
  if (config.grvtSignerAddress) {
    const providedAddress = config.grvtSignerAddress.toLowerCase();
    const walletAddress = wallet.address.toLowerCase();

    if (providedAddress === walletAddress) {
      console.log('✅ Provided signer address matches wallet address');
    } else {
      console.log('\n⚠️  WARNING: Mismatch detected!');
      console.log('  Provided in config:', config.grvtSignerAddress);
      console.log('  Derived from private key:', wallet.address);
      console.log('\n  The derived address is what matters.');
      console.log('  Update your .apikey.json with the correct address.\n');
    }
  }
} catch (error: any) {
  console.error('❌ Error:', error.message);
  process.exit(1);
}
