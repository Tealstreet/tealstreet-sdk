import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the table render-mode setter (the only runtime dep)
vi.mock('../table/Table', () => ({
  setTableRenderMode: vi.fn(),
}));

import { WorkerPlatformAdapter } from './WorkerPlatformAdapter';
import { setTableRenderMode } from '../table/Table';
import type { ExchangeLike, ExchangeAccountConfig } from '../types';

describe('WorkerPlatformAdapter', () => {
  let adapter: WorkerPlatformAdapter;
  let mockExchange: ExchangeLike;
  let mockEmit: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEmit = vi.fn();
    mockExchange = {
      emitter: {
        emit: mockEmit,
      },
    } as unknown as ExchangeLike;

    adapter = new WorkerPlatformAdapter(mockExchange);
  });

  describe('constructor', () => {
    it('should set table render mode to web', () => {
      expect(setTableRenderMode).toHaveBeenCalledWith('web');
    });

    it('should store the exchange reference', () => {
      // This is implicit in the fact that print methods work
      adapter.print(['test']);
      expect(mockEmit).toHaveBeenCalled();
    });
  });

  describe('account management methods (stubs)', () => {
    describe('listAccounts', () => {
      it('should return empty array', () => {
        expect(adapter.listAccounts()).toEqual([]);
      });
    });

    describe('getAccount', () => {
      it('should return undefined', () => {
        expect(adapter.getAccount('test')).toBeUndefined();
      });

      it('should return undefined when called without argument', () => {
        expect(adapter.getAccount()).toBeUndefined();
      });
    });

    describe('addAccount', () => {
      it('should be a no-op', () => {
        const account: ExchangeAccountConfig = {
          name: 'test',
          exchange: 'bybit',
          apiKey: 'key',
          apiSecret: 'secret',
        };
        // Should not throw
        expect(() => adapter.addAccount(account)).not.toThrow();
      });
    });

    describe('updateAccount', () => {
      it('should return false', () => {
        expect(adapter.updateAccount('test', { name: 'updated' })).toBe(false);
      });
    });

    describe('removeAccount', () => {
      it('should return false', () => {
        expect(adapter.removeAccount('test')).toBe(false);
      });
    });

    describe('setDefaultAccount', () => {
      it('should return false', () => {
        expect(adapter.setDefaultAccount('test')).toBe(false);
      });
    });

    describe('getDefaultSymbol', () => {
      it('should return undefined', () => {
        expect(adapter.getDefaultSymbol()).toBeUndefined();
      });
    });

    describe('setDefaultSymbol', () => {
      it('should be a no-op', () => {
        expect(() => adapter.setDefaultSymbol('BTCUSDT')).not.toThrow();
      });
    });

    describe('getActiveAccounts', () => {
      it('should return empty array', () => {
        expect(adapter.getActiveAccounts()).toEqual([]);
      });
    });
  });

  describe('exchange management methods (stubs)', () => {
    describe('createExchange', () => {
      it('should return the existing exchange', async () => {
        const account: ExchangeAccountConfig = {
          name: 'test',
          exchange: 'bybit',
          apiKey: 'key',
          apiSecret: 'secret',
        };
        const result = await adapter.createExchange(account);
        expect(result).toBe(mockExchange);
      });
    });

    describe('startExchange', () => {
      it('should be a no-op', async () => {
        await expect(adapter.startExchange(mockExchange)).resolves.toBeUndefined();
      });
    });

    describe('stopExchange', () => {
      it('should be a no-op', async () => {
        await expect(adapter.stopExchange(mockExchange)).resolves.toBeUndefined();
      });
    });
  });

  describe('output methods', () => {
    describe('print', () => {
      it('should emit exec-log with joined lines', () => {
        adapter.print(['line1', 'line2', 'line3']);
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'line1\nline2\nline3',
        });
      });

      it('should handle single line', () => {
        adapter.print(['single line']);
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'single line',
        });
      });

      it('should handle empty array', () => {
        adapter.print([]);
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: '',
        });
      });
    });

    describe('printError', () => {
      it('should emit exec-log with Error prefix', () => {
        adapter.printError('Something went wrong');
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Error: Something went wrong',
        });
      });
    });

    describe('printSuccess', () => {
      it('should emit exec-log with message', () => {
        adapter.printSuccess('Operation completed');
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Operation completed',
        });
      });
    });

    describe('printWarning', () => {
      it('should emit exec-log with Warning prefix', () => {
        adapter.printWarning('This might be an issue');
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Warning: This might be an issue',
        });
      });
    });

    describe('printInfo', () => {
      it('should emit exec-log with message', () => {
        adapter.printInfo('Some info');
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: 'Some info',
        });
      });
    });
  });
});
