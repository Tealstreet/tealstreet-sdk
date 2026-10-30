import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { CommandExecutor } from '../commandExecutor';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { registerCommandExecutor, resetCommandExecutor } from '../commandExecutor';
import { CommandHandler } from './CommandHandler';

describe('CommandHandler', () => {
  let handler: CommandHandler;
  let mockExchange: Exchange;
  let mockEmit: ReturnType<typeof vi.fn>;
  let mockExecutor: ReturnType<typeof vi.fn>;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;

  beforeEach(() => {
    vi.clearAllMocks();

    mockEmit = vi.fn();
    mockExchange = {
      name: 'bybit',
      emitter: { emit: mockEmit },
    } as unknown as Exchange;
    mockCtx = {} as Worker;
    mockSettings = {
      placeOrder: {
        tpTriggerType: OrderStopTriggerType.MarkPrice,
        slTriggerType: OrderStopTriggerType.LastPrice,
      },
    };
    mockMetadata = {
      supportedTPSLTriggerTypes: [],
      privateProxyIps: [],
      defaultTPTriggerType: undefined,
      defaultSLTriggerType: undefined,
    };

    // Inject a stub executor (the role the web app's cli-core-backed
    // workerCommandExecutor fills at runtime). Resolves by default — the real
    // executor returns a Promise, which the handler attaches .catch to.
    mockExecutor = vi.fn().mockResolvedValue(undefined);
    registerCommandExecutor(mockExecutor as unknown as CommandExecutor);

    handler = new CommandHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  afterEach(() => {
    resetCommandExecutor();
  });

  describe('handle', () => {
    it('should call the registered executor for exec op', () => {
      const payload: StoreWorkerPayload = {
        op: 'exec',
        data: {
          symbol: 'BTCUSDT',
          command: 'buy 1',
        },
      };

      handler.handle(payload);

      // reqId is undefined when the CLI didn't request a result echo
      // (the common fire-and-forget case). The executor takes it as the
      // optional 4th arg.
      expect(mockExecutor).toHaveBeenCalledWith(mockExchange, 'BTCUSDT', 'buy 1', undefined);
    });

    it('should pass symbol and command correctly', () => {
      const payload: StoreWorkerPayload = {
        op: 'exec',
        data: {
          symbol: 'ETHUSDT',
          command: 'sell 2.5; buy 1',
        },
      };

      handler.handle(payload);

      expect(mockExecutor).toHaveBeenCalledWith(mockExchange, 'ETHUSDT', 'sell 2.5; buy 1', undefined);
    });

    it('should forward reqId when the CLI requested a result echo', () => {
      const payload: StoreWorkerPayload = {
        op: 'exec',
        data: {
          symbol: 'BTCUSDT',
          command: 'buy 1',
          reqId: 'req-abc-123',
        },
      };

      handler.handle(payload);

      expect(mockExecutor).toHaveBeenCalledWith(mockExchange, 'BTCUSDT', 'buy 1', 'req-abc-123');
    });

    it('should not call the executor for other ops', () => {
      const payload: StoreWorkerPayload = {
        op: 'some-other-op' as any,
        data: {
          symbol: 'BTCUSDT',
          command: 'buy 1',
        },
      };

      handler.handle(payload);

      expect(mockExecutor).not.toHaveBeenCalled();
    });

    it('logs and does not crash for malformed exec payloads', () => {
      expect(() => handler.handle({ op: 'exec' } as StoreWorkerPayload)).not.toThrow();

      expect(mockExecutor).not.toHaveBeenCalled();
      expect(mockEmit).toHaveBeenCalledWith('exec-log', {
        message: 'Invalid command payload.',
      });
    });

    it('should handle multiple exec calls', () => {
      const payload1: StoreWorkerPayload = {
        op: 'exec',
        data: { symbol: 'BTCUSDT', command: 'buy 1' },
      };
      const payload2: StoreWorkerPayload = {
        op: 'exec',
        data: { symbol: 'ETHUSDT', command: 'sell 2' },
      };

      handler.handle(payload1);
      handler.handle(payload2);

      expect(mockExecutor).toHaveBeenCalledTimes(2);
    });

    it('no-ops with a log when no executor is registered', () => {
      resetCommandExecutor();

      const payload: StoreWorkerPayload = {
        op: 'exec',
        data: { symbol: 'BTCUSDT', command: 'buy 1' },
      };

      expect(() => handler.handle(payload)).not.toThrow();
      expect(mockExecutor).not.toHaveBeenCalled();
      expect(mockEmit).toHaveBeenCalledWith('exec-log', {
        message: 'Command execution is not available in this environment.',
      });
    });

    it('logs and does not crash when the executor rejects', async () => {
      mockExecutor.mockRejectedValueOnce(new Error('boom'));

      const payload: StoreWorkerPayload = {
        op: 'exec',
        data: { symbol: 'BTCUSDT', command: 'buy 1' },
      };

      expect(() => handler.handle(payload)).not.toThrow();
      // The rejection is handled asynchronously via .catch -> exec-log.
      await vi.waitFor(() => {
        expect(mockEmit).toHaveBeenCalledWith('exec-log', {
          message: expect.stringContaining('boom'),
        });
      });
    });

    it('answers a reqId with a failed exec-result when the executor throws', async () => {
      mockExecutor.mockRejectedValueOnce(new Error('boom'));
      handler.handle({ op: 'exec', data: { symbol: 'BTCUSDT', command: 'buy 1', reqId: 'cb:9' } });
      await vi.waitFor(() => {
        expect(mockEmit).toHaveBeenCalledWith('exec-log', { message: expect.stringContaining('boom'), reqId: 'cb:9' });
        expect(mockEmit).toHaveBeenCalledWith('exec-result', {
          reqId: 'cb:9',
          handled: true,
          success: false,
          error: expect.stringContaining('boom'),
          errorDetails: { message: 'boom', name: 'Error', stack: expect.any(String) },
        });
      });
    });
  });
});
