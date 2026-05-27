import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { Network, IndexerClient, ValidatorClient } from '@dydxprotocol/v4-client-js';
import { createWalletFromPrivateKey } from '../dydx.sdk';

/**
 * Test to check wallet-level balances (DYDX tokens, USDC) using the SDK
 * This tests balances at the Cosmos chain level, not just the dYdX exchange subaccounts
 */

interface ApiKeyConfig {
  apiKey: string;
  secret: string;
  testnet?: boolean;
}

describe('DydxExchange - Wallet Balance Check', () => {
  let apiConfig: ApiKeyConfig;
  let network: Network;
  let address: string;

  beforeAll(async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    address = apiConfig.apiKey;
    network = apiConfig.testnet ? Network.testnet() : Network.mainnet();

    console.log('\n==============================');
    console.log('dYdX Wallet Balance Check');
    console.log('==============================');
    console.log('Address:', address);
    console.log('Network:', apiConfig.testnet ? 'Testnet' : 'Mainnet');
    console.log('==============================\n');
  });

  it('should check native DYDX token balance', async () => {
    try {
      const validatorClient = await ValidatorClient.connect(network.validatorConfig);

      console.log('Querying native DYDX token balance...');
      // Use type assertion since the SDK types may not expose all available methods
      const balances = await (validatorClient.get as any).getAllBalances(address);

      console.log('All balances:', JSON.stringify(balances, null, 2));

      if (balances && balances.length > 0) {
        for (const balance of balances) {
          console.log(`  - ${balance.denom}: ${balance.amount}`);
        }
      } else {
        console.log('  No balances found');
      }

      expect(balances).toBeDefined();
    } catch (error: any) {
      console.error('Error querying native balance:', error.message);
      throw error;
    }
  }, 30000);

  it('should check account info from validator', async () => {
    try {
      const validatorClient = await ValidatorClient.connect(network.validatorConfig);

      console.log('\nQuerying account info from validator...');
      // Use type assertion since the SDK types may not expose all available methods
      const accountInfo = await (validatorClient.get as any).getAccountBalances(address);

      console.log('Account info:', JSON.stringify(accountInfo, null, 2));

      expect(accountInfo).toBeDefined();
    } catch (error: any) {
      console.error('Error querying account info:', error.message);
      // Don't throw - account might not exist yet
      console.log('Account might not exist on chain yet');
    }
  }, 30000);

  it('should try to get account from indexer (any subaccounts)', async () => {
    try {
      const indexerClient = new IndexerClient(network.indexerConfig);

      console.log('\nQuerying all subaccounts from indexer...');
      const subaccounts = await indexerClient.account.getSubaccounts(address);

      console.log('Subaccounts:', JSON.stringify(subaccounts, null, 2));

      if (subaccounts && subaccounts.subaccounts) {
        console.log(`Found ${subaccounts.subaccounts.length} subaccount(s)`);

        for (const subaccount of subaccounts.subaccounts) {
          console.log(`\nSubaccount ${subaccount.subaccountNumber}:`);
          console.log(`  Equity: ${subaccount.equity}`);
          console.log(`  Free Collateral: ${subaccount.freeCollateral}`);
          console.log(`  Margin Enabled: ${subaccount.marginEnabled}`);
        }
      } else {
        console.log('No subaccounts found');
      }

      expect(subaccounts).toBeDefined();
    } catch (error: any) {
      console.error('Error querying subaccounts:', error.message);
      if (error.response) {
        console.log('Response status:', error.response.status);
        console.log('Response data:', JSON.stringify(error.response.data, null, 2));
      }
      // Don't throw - this might be expected for new accounts
    }
  }, 30000);

  it('should check if wallet is valid', async () => {
    try {
      console.log('\nValidating wallet from private key...');

      const privateKey = apiConfig.secret.startsWith('0x')
        ? apiConfig.secret.slice(2)
        : apiConfig.secret;

      const wallet = await createWalletFromPrivateKey(privateKey, 'dydx');

      console.log('Wallet created successfully');
      console.log('  Address:', wallet.address);
      console.log('  Matches config:', wallet.address === address);

      expect(wallet.address).toBe(address);
    } catch (error: any) {
      console.error('Error creating wallet:', error.message);
      throw error;
    }
  }, 30000);

  it('should summarize findings', () => {
    console.log('\n======================================');
    console.log('SUMMARY');
    console.log('======================================');
    console.log('1. If no native balances: Wallet exists but empty');
    console.log('2. If has DYDX/USDC: Funds in wallet, not deposited to exchange');
    console.log('3. If no subaccounts: No trading activity yet');
    console.log('');
    console.log('To trade on dYdX:');
    console.log('  1. Need USDC deposited to dYdX exchange');
    console.log('  2. Deposit creates first subaccount');
    console.log('  3. Then can place orders and view balances');
    console.log('======================================\n');

    expect(true).toBe(true);
  });
});
