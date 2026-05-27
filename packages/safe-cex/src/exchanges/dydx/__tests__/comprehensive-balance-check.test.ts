import { describe, it, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { Network, IndexerClient } from '@dydxprotocol/v4-client-js';
import { createWalletFromPrivateKey } from '../dydx.sdk';

/**
 * Comprehensive test to find balance on dYdX account
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

describe('DydxExchange - Comprehensive Balance Check', () => {
  let apiConfig: ApiKeyConfig;
  let network: Network;
  let address: string;
  let indexerClient: IndexerClient;

  beforeAll(async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    address = apiConfig.apiKey;
    network = apiConfig.testnet ? Network.testnet() : Network.mainnet();
    indexerClient = new IndexerClient(network.indexerConfig);

    console.log('\n==============================');
    console.log('dYdX Comprehensive Balance Check');
    console.log('==============================');
    console.log('Address:', address);
    console.log('Network:', apiConfig.testnet ? 'Testnet' : 'Mainnet');
    console.log('==============================\n');
  });

  it('should verify address matches private key', async () => {
    const privateKey = apiConfig.secret.startsWith('0x')
      ? apiConfig.secret.slice(2)
      : apiConfig.secret;

    const wallet = await createWalletFromPrivateKey(privateKey, 'dydx');

    console.log('Configured address:', address);
    console.log('Derived address:  ', wallet.address);
    console.log('Match:', wallet.address === address);

    if (wallet.address !== address) {
      console.log('❌ ADDRESS MISMATCH! This could be the issue.');
    } else {
      console.log('✅ Address matches private key');
    }
  }, 30000);

  it('should check all subaccounts (0-10)', async () => {
    console.log('\n--- Checking subaccounts 0-10 ---\n');

    for (let subaccountNumber = 0; subaccountNumber <= 10; subaccountNumber++) {
      try {
        const subaccount = await indexerClient.account.getSubaccount(address, subaccountNumber);

        console.log(`✅ FOUND Subaccount ${subaccountNumber}:`);
        console.log(JSON.stringify(subaccount, null, 2));
      } catch (error: any) {
        if (error.response?.status === 404) {
          console.log(`   Subaccount ${subaccountNumber}: Not found (404)`);
        } else {
          console.log(`   Subaccount ${subaccountNumber}: Error - ${error.message}`);
        }
      }
    }
  }, 60000);

  it('should get all subaccounts via getSubaccounts', async () => {
    console.log('\n--- Getting all subaccounts via getSubaccounts() ---\n');

    try {
      const result = await indexerClient.account.getSubaccounts(address);

      console.log('Result:', JSON.stringify(result, null, 2));

      if (result?.subaccounts && result.subaccounts.length > 0) {
        console.log(`\n✅ Found ${result.subaccounts.length} subaccount(s):`);
        for (const sub of result.subaccounts) {
          console.log(`\nSubaccount ${sub.subaccountNumber}:`);
          console.log(`  Equity: ${sub.equity}`);
          console.log(`  Free Collateral: ${sub.freeCollateral}`);
          console.log(`  Margin Enabled: ${sub.marginEnabled}`);
        }
      } else {
        console.log('⚠️  No subaccounts found');
      }
    } catch (error: any) {
      console.log('Error:', error.message);
      if (error.response) {
        console.log('Status:', error.response.status);
        console.log('Data:', JSON.stringify(error.response.data, null, 2));
      }
    }
  }, 30000);

  it('should check asset positions for subaccount 0', async () => {
    console.log('\n--- Checking asset positions ---\n');

    try {
      const positions = await indexerClient.account.getSubaccountAssetPositions(address, 0);

      console.log('Asset positions:', JSON.stringify(positions, null, 2));

      if (positions?.positions) {
        for (const pos of positions.positions) {
          console.log(`\n${pos.symbol}: ${pos.size}`);
        }
      }
    } catch (error: any) {
      console.log('Error:', error.message);
      if (error.response?.status === 404) {
        console.log('404 - No asset positions found');
      }
    }
  }, 30000);

  it('should check perpetual positions', async () => {
    console.log('\n--- Checking perpetual positions ---\n');

    try {
      const positions = await indexerClient.account.getSubaccountPerpetualPositions(address, 0);

      console.log('Perpetual positions:', JSON.stringify(positions, null, 2));

      if (positions?.positions) {
        console.log(`Found ${positions.positions.length} position(s)`);
        for (const pos of positions.positions) {
          console.log(`\n${pos.market}:`);
          console.log(`  Size: ${pos.size}`);
          console.log(`  Entry Price: ${pos.entryPrice}`);
          console.log(`  Unrealized PnL: ${pos.unrealizedPnl}`);
        }
      }
    } catch (error: any) {
      console.log('Error:', error.message);
      if (error.response?.status === 404) {
        console.log('404 - No perpetual positions found');
      }
    }
  }, 30000);
});
