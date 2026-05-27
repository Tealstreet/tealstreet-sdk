import { resolve } from 'path';
import { existsSync, readFileSync } from 'fs';
import { ethers } from 'ethers';

/**
 * Helper script to display the signer address that needs to be tagged in GRVT UI
 *
 * GRVT requires the Ethereum address (derived from your private key) to be tagged
 * to your API key in the GRVT web interface before you can access trading endpoints.
 */

async function main() {
  console.log('\n📋 GRVT Signer Address Checker\n');
  console.log('='.repeat(60));

  // Load credentials
  const credPath = resolve(__dirname, '.apikey.json');
  if (!existsSync(credPath)) {
    console.log('❌ No .apikey.json file found');
    console.log('💡 Create a .apikey.json file with your GRVT credentials');
    return;
  }

  let credentials: any;
  try {
    credentials = JSON.parse(readFileSync(credPath, 'utf-8'));
  } catch (error) {
    console.log(`❌ Error loading credentials: ${error}`);
    return;
  }

  if (!credentials.secret) {
    console.log('❌ No private key found in .apikey.json');
    console.log('💡 Add your Ethereum private key to the "secret" field');
    return;
  }

  // Derive signer address from private key
  let wallet: ethers.Wallet;
  try {
    wallet = new ethers.Wallet(credentials.secret);
  } catch (error: any) {
    console.log(`❌ Invalid private key: ${error.message}`);
    return;
  }

  console.log('\n✅ Signer Address Information:\n');
  console.log(`   Ethereum Address: ${wallet.address}`);
  console.log(`   API Key: ${credentials.apiKey || 'Not found'}`);
  console.log(`   Environment: ${credentials.testnet !== false ? 'Testnet' : 'Mainnet'}`);

  const grvtUrl = credentials.testnet !== false
    ? 'https://testnet.grvt.io/exchange/account/api-keys'
    : 'https://grvt.io/exchange/account/api-keys';

  console.log('\n' + '='.repeat(60));
  console.log('\n⚠️  ACTION REQUIRED:\n');
  console.log('To fix error code 1001 ("You are not authorized to access this functionality"):');
  console.log('\n1. Go to GRVT API Keys page:');
  console.log(`   ${grvtUrl}`);
  console.log('\n2. Find your API key or create a new one');
  console.log('\n3. Tag/link this Ethereum address to your API key:');
  console.log(`   ${wallet.address}`);
  console.log('\n4. Ensure the API key has "Trading" permissions enabled');
  console.log('\n5. Save and try again\n');
  console.log('='.repeat(60) + '\n');
}

main();
