import { describe, expect, it } from 'vitest';

import type { CLIPlatformAdapter, CLISession } from '@tealstreet/cli-core';

import type { ExchangeManager } from './exchange-manager';
import { createStandaloneCoreAdapter } from './script-adapter';

const buildPlatformAdapter = (): CLIPlatformAdapter =>
  ({
    listAccounts: () => [],
    getAccount: () => undefined,
    addAccount: () => {},
    updateAccount: () => false,
    removeAccount: () => false,
    setDefaultAccount: () => false,
    getDefaultSymbol: () => undefined,
    setDefaultSymbol: () => {},
    getActiveAccounts: () => [],
    createExchange: async () => {
      throw new Error('not implemented');
    },
    startExchange: async () => {},
    stopExchange: async () => {},
    print: () => {},
    printError: () => {},
    printSuccess: () => {},
    printWarning: () => {},
    printInfo: () => {},
  }) as CLIPlatformAdapter;

describe('createStandaloneCoreAdapter', () => {
  it('fails script runCommand calls that cli-core does not handle', async () => {
    const exchangeManager = {
      getExchange: () => null,
    } as unknown as ExchangeManager;
    const session: CLISession = {
      currentExchange: null,
      currentAccountName: 'acct',
      currentSymbol: 'BTCUSDT',
    };

    const adapter = createStandaloneCoreAdapter(exchangeManager, session, buildPlatformAdapter());
    const result = await adapter.runCommand('script:missing', { account: 'acct', depth: 1 });

    expect(result).toEqual({
      ok: false,
      capturedIds: [],
      output: [],
      error: 'Unknown command: script:missing',
    });
  });
});
