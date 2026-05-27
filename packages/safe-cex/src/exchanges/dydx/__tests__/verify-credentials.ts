/**
 * Diagnostic script to verify dYdX credentials
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import { LocalWallet } from '@dydxprotocol/v4-client-js';

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet: boolean;
  dydxAddress?: string;
}

async function verifyCredentials() {
  console.log('\n===========================================');
  console.log('🔍 dYdX Credentials Verification');
  console.log('===========================================\n');

  // Load credentials
  const configPath = join(__dirname, '..', '.apikey.json');
  const config: ApiKeyConfig = JSON.parse(readFileSync(configPath, 'utf-8'));

  console.log('📋 Configuration:');
  console.log(`  API Key (address): ${config.apiKey}`);
  console.log(`  dydxAddress: ${config.dydxAddress || 'Not set'}`);
  console.log(`  Network: ${config.testnet ? 'Testnet' : 'Mainnet'}`);
  console.log(`  Secret format: ${config.secret.startsWith('0x') ? 'Private Key (hex)' : 'Mnemonic or other'}\n`);

  try {
    // Try to create wallet from private key
    const privateKey = config.secret.startsWith('0x') ? config.secret.slice(2) : config.secret;

    console.log('🔐 Attempting to create wallet from private key...');
    const wallet = await LocalWallet.fromPrivateKey(privateKey, 'dydx');

    console.log('✅ Wallet created successfully!');
    console.log(`  Derived address: ${wallet.address}\n`);

    // Check for address mismatches
    console.log('🔍 Address Verification:');

    if (wallet.address === config.apiKey) {
      console.log('  ✅ Derived address matches apiKey');
    } else {
      console.log('  ❌ Derived address DOES NOT match apiKey');
      console.log(`     Expected: ${config.apiKey}`);
      console.log(`     Got:      ${wallet.address}`);
    }

    if (config.dydxAddress) {
      if (wallet.address === config.dydxAddress) {
        console.log('  ✅ Derived address matches dydxAddress');
      } else {
        console.log('  ❌ Derived address DOES NOT match dydxAddress');
        console.log(`     Expected: ${config.dydxAddress}`);
        console.log(`     Got:      ${wallet.address}`);
      }
    }

    console.log('\n📊 Summary:');
    if (wallet.address === config.apiKey && (!config.dydxAddress || wallet.address === config.dydxAddress)) {
      console.log('  ✅ All addresses match! Credentials should work.');
    } else {
      console.log('  ⚠️  Address mismatch detected!');
      console.log('\n💡 Recommendations:');
      console.log('  1. Update apiKey to match derived address: ' + wallet.address);
      if (config.dydxAddress && wallet.address !== config.dydxAddress) {
        console.log('  2. Either remove dydxAddress or ensure you have the correct private key for it');
        console.log('  3. If using a separate trading wallet, you need BOTH wallets\' private keys');
      }
    }

    console.log('\n===========================================\n');

  } catch (error: any) {
    console.error('❌ Error creating wallet:');
    console.error(error.message);
    console.error('\n💡 This might indicate:');
    console.error('  - Invalid private key format');
    console.error('  - The secret might be a mnemonic (use 12-24 words instead)');
    console.error('  - Corrupted credentials file\n');
  }
}

verifyCredentials().catch(console.error);
