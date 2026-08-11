import { describe, expect, it, vi } from 'vitest';

import type { CommandContext } from '../Command';
import { parseArgs } from '../parseArgs';
import { AccountsCommand } from './AccountsCommand';

function createContext(raw: string): CommandContext {
  return {
    raw,
    args: parseArgs(raw),
    symbol: 'BTCUSDT',
    accountName: undefined,
    exchange: null,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      listAccounts: vi.fn().mockReturnValue([]),
      getAccount: vi.fn(),
      addAccount: vi.fn(),
      updateAccount: vi.fn(),
      removeAccount: vi.fn(),
      setDefaultAccount: vi.fn(),
      getDefaultSymbol: vi.fn(),
      setDefaultSymbol: vi.fn(),
      getActiveAccounts: vi.fn().mockReturnValue([]),
      createExchange: vi.fn(),
      startExchange: vi.fn(),
      stopExchange: vi.fn(),
    },
  };
}

describe('AccountsCommand', () => {
  it('normalizes manual add accounts with explicit Tealstreet proxy flags', async () => {
    const command = new AccountsCommand();
    const ctx = createContext('accounts add -n mybybit -e bybit -k key -s secret --tealstreet-proxy --proxy-region JA');

    await command.execute(ctx);

    expect(ctx.adapter.addAccount).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'mybybit',
        exchange: 'bybit',
        tealstreetProxy: true,
        proxyRegion: 'JA',
      }),
    );
  });

  it('normalizes imported web account JSON', async () => {
    const command = new AccountsCommand();
    const ctx = createContext(
      'accounts import {"exchange":"bybit","name":"mybybit","apiKey":"key","apiSecret":"secret","tealstreetProxy":true}',
    );

    await command.execute(ctx);

    expect(ctx.adapter.addAccount).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'mybybit',
        exchange: 'bybit',
        tealstreetProxy: true,
        proxyRegion: 'SG',
      }),
    );
  });

  it('adds Paradex accounts with canonical web credential fields', async () => {
    const command = new AccountsCommand();
    const ctx = createContext(
      'accounts add -n pd -e paradex -k 0xsubkey-private --paradex-address 0xmain --subkey-address 0xsubkey',
    );

    await command.execute(ctx);

    expect(ctx.adapter.addAccount).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'pd',
        exchange: 'paradex',
        apiKey: '0xsubkey-private',
        apiSecret: '0xsubkey-private',
        paradexAddress: '0xmain',
        subkeyAddress: '0xsubkey',
      }),
    );
  });

  it('rejects incomplete Paradex account adds before persistence', async () => {
    const command = new AccountsCommand();
    const ctx = createContext('accounts add -n pd -e paradex -k 0xsubkey-private --paradex-address 0xmain');

    await command.execute(ctx);

    expect(ctx.adapter.addAccount).not.toHaveBeenCalled();
    expect(ctx.adapter.printError).toHaveBeenCalledWith(
      expect.stringContaining('Missing required fields for Paradex: subkeyAddress'),
    );
  });

  it('imports Paradex web account JSON without dropping address fields', async () => {
    const command = new AccountsCommand();
    const ctx = createContext(
      'accounts import {"exchange":"paradex","name":"pd","apiKey":"0xsubkey-private","apiSecret":"0xsubkey-private","paradexAddress":"0xmain","subkeyAddress":"0xsubkey"}',
    );

    await command.execute(ctx);

    expect(ctx.adapter.addAccount).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'pd',
        exchange: 'paradex',
        apiKey: '0xsubkey-private',
        apiSecret: '0xsubkey-private',
        paradexAddress: '0xmain',
        subkeyAddress: '0xsubkey',
      }),
    );
  });
});
