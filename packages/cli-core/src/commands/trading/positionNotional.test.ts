import { describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { PositionSide, type Position } from '@tealstreet/safe-cex/src/types';

import type { CLIPlatformAdapter, ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import { preprocessSegment } from '../../preprocess/substitute';
import { PositionCommand } from './PositionCommand';
import { PositionsCommand } from './PositionsCommand';
import {
  formatMarginTable,
  formatMultiAccountMarginTable,
  formatMultiAccountPositionsTable,
  formatPositionsTable,
} from './MonitorCommand';

function fixture(notional: number | null) {
  const position: Position = JSON.parse(
    JSON.stringify({
      id: 'p',
      symbol: 'PF_XBTUSD',
      side: PositionSide.Long,
      entryPrice: 100,
      notional,
      contracts: 2,
      leverage: 5,
      unrealizedPnl: 10,
      liquidationPrice: null,
    }),
  );
  const store = new DefaultStore();
  store.update({
    positions: [position],
    balance: { ...store.balance, total: 1000, free: 1000, used: 0 },
  });
  const exchange: ExchangeLike = {
    name: 'kraken',
    store,
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  };
  const print = vi.fn();
  const adapter: CLIPlatformAdapter = {
    listAccounts: () => [],
    getAccount: () => undefined,
    addAccount: vi.fn(),
    updateAccount: () => false,
    removeAccount: () => false,
    setDefaultAccount: () => false,
    getDefaultSymbol: () => undefined,
    setDefaultSymbol: vi.fn(),
    getActiveAccounts: () => [],
    createExchange: async () => exchange,
    startExchange: async () => {},
    stopExchange: async () => {},
    print,
    printError: vi.fn(),
    printSuccess: vi.fn(),
    printWarning: vi.fn(),
    printInfo: vi.fn(),
  };
  return { store, exchange, adapter, print };
}

const plain = (value: string) => value.replace(/\x1b\[[0-9;]*m/g, '');

describe('position notional across JSON consumers', () => {
  describe.each([
    { name: 'positions', render: (exchange: ExchangeLike) => formatPositionsTable(exchange) },
    { name: 'margin', render: (exchange: ExchangeLike) => formatMarginTable(exchange) },
    {
      name: 'multi positions',
      render: (exchange: ExchangeLike) =>
        formatMultiAccountPositionsTable([
          { accountName: 'unknown', exchange },
          { accountName: 'known', exchange: fixture(250).exchange },
        ]),
    },
    {
      name: 'multi margin',
      render: (exchange: ExchangeLike) =>
        formatMultiAccountMarginTable([
          { accountName: 'unknown', exchange },
          { accountName: 'known', exchange: fixture(250).exchange },
        ]),
    },
  ])('$name', ({ name, render }) => {
    it.each([null, NaN, Infinity, -Infinity])('shows %s as unknown rather than a partial total or zero', (notional) => {
      const { store, exchange } = fixture(notional);
      store.update({
        positions: [...store.positions, { ...store.positions[0], id: 'known', symbol: 'ETHUSDT', notional: 250 }],
      });
      const output = plain(render(exchange));
      expect(output).toContain('—');
      expect(output).not.toMatch(/NaN|Infinity/);
      const lines = output.split('\n');
      if (name === 'margin') {
        for (const label of ['Position Size', 'Account Lev', 'Margin Ratio']) {
          expect(lines.find((line) => line.includes(label))).toContain('—');
        }
      } else if (name === 'multi margin') {
        for (const account of ['unknown', 'TOTAL']) {
          expect(lines.find((line) => line.includes(account))?.match(/—/g)).toHaveLength(2);
        }
        expect(lines.find((line) => line.includes('known') && !line.includes('unknown'))).not.toContain('—');
      } else {
        const unknownRow = lines.find(
          (line) => line.includes('PF_XBTUSD') && (name !== 'multi positions' || line.includes('unknown')),
        );
        expect(unknownRow?.match(/—/g)).toHaveLength(2);
        expect(lines.find((line) => line.includes('ETHUSDT'))).not.toContain('—');
      }
    });
  });

  describe.each([
    { name: 'position', create: () => new PositionCommand() },
    { name: 'positions', create: () => new PositionsCommand() },
  ])('$name command', ({ create }) => {
    it.each([0, 250])('selects open quantity with known notional %s', async (notional) => {
      const { exchange, adapter, print } = fixture(notional);
      const command = create();
      await command.execute({ raw: command.name, args: { _: [command.name] }, symbol: 'PF_XBTUSD', exchange, adapter });
      expect(print.mock.calls.flat(2).join('\n')).toContain('PF_XBTUSD');
    });
    it('does not guess an unknown percentage or account leverage', async () => {
      const { exchange, adapter, print } = fixture(NaN);
      const command = create();
      await command.execute({ raw: command.name, args: { _: [command.name] }, symbol: 'PF_XBTUSD', exchange, adapter });
      const output = print.mock.calls.flat(2).join('\n');
      expect(output).toContain('PF_XBTUSD');
      expect(output).toContain('—');
      expect(output).not.toMatch(/NaN|Infinity|0\.00x|0\.0+%/);
    });
  });

  it.each([0, 250])('keeps known %s exposures and leverage numeric', (notional) => {
    const { exchange } = fixture(notional);
    expect(plain(formatMarginTable(exchange))).not.toContain('—');
    expect(plain(formatMultiAccountMarginTable([{ accountName: 'known', exchange }]))).not.toContain('—');
  });

  it.each(['$totallongusd', '$totalshortusd', '$totallong', '$totalshort', '$margin'])(
    'refuses the %s sizing token when a required position notional is unknown',
    (token) => {
      const { store, exchange } = fixture(NaN);
      store.update({
        positions: [...store.positions, { ...store.positions[0], id: 'short', side: PositionSide.Short }],
      });
      expect(() =>
        preprocessSegment(`buy ${token}`, {
          exchange,
          symbol: 'PF_XBTUSD',
          store: new MemoryPreprocessStore(),
        }),
      ).toThrow(/notional.*unknown/i);
    },
  );

  it('keeps known token values and empty exposure scopes numeric', () => {
    const { store, exchange } = fixture(250);
    const ctx = { exchange, symbol: 'PF_XBTUSD', store: new MemoryPreprocessStore() };
    for (const [token, value] of [
      ['$totallongusd', 250],
      ['$totalshortusd', 0],
      ['$totallong', 0.25],
      ['$margin', 50],
    ]) {
      expect(preprocessSegment(`buy ${token}`, ctx)).toBe(`buy ${value}`);
    }
    store.update({ positions: [] });
    expect(preprocessSegment('buy $totallongusd', ctx)).toBe('buy 0');
  });
});
