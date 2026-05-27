import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { howWebSocketJA, howWebSocketSG } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { OrderStopTriggerType } from '../../types';
import { DevHandler } from './DevHandler';

// Mock the HOW websocket
vi.mock('../../httpOverWebsocket/axiosHttpOverWebsocketAdapter', () => ({
  howWebSocketJA: {
    simulateClose: vi.fn(),
  },
  howWebSocketSG: {
    simulateClose: vi.fn(),
  },
}));

describe('DevHandler', () => {
  let handler: DevHandler;
  let mockExchange: Exchange;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;
  let mockReconnect: ReturnType<typeof vi.fn>;
  let mockForceOpen: ReturnType<typeof vi.fn>;
  let mockForceClose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();

    mockReconnect = vi.fn();
    mockForceOpen = vi.fn();
    mockForceClose = vi.fn();

    const mockSocket = {
      reconnect: mockReconnect,
      forceOpen: mockForceOpen,
      forceClose: mockForceClose,
    };

    mockExchange = {
      name: 'bybit',
      getAllSockets: vi.fn().mockReturnValue([mockSocket, mockSocket]),
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

    handler = new DevHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('dev:simulateWebSocketsReconnect', () => {
      it('should call reconnect on all sockets', () => {
        const payload: StoreWorkerPayload = {
          op: 'dev:simulateWebSocketsReconnect',
        };

        handler.handle(payload);

        expect(mockExchange.getAllSockets).toHaveBeenCalled();
        expect(mockReconnect).toHaveBeenCalledTimes(2);
      });
    });

    describe('dev:simulateHOWClose', () => {
      it('should call simulateClose on both HOW websockets', () => {
        const payload: StoreWorkerPayload = {
          op: 'dev:simulateHOWClose',
        };

        handler.handle(payload);

        expect(vi.mocked(howWebSocketJA).simulateClose).toHaveBeenCalledTimes(1);
        expect(vi.mocked(howWebSocketSG).simulateClose).toHaveBeenCalledTimes(1);
      });
    });

    describe('dev:forceOpen', () => {
      it('should call forceOpen on all sockets', () => {
        const payload: StoreWorkerPayload = {
          op: 'dev:forceOpen',
        };

        handler.handle(payload);

        expect(mockExchange.getAllSockets).toHaveBeenCalled();
        expect(mockForceOpen).toHaveBeenCalledTimes(2);
      });
    });

    describe('dev:forceClose', () => {
      it('should call forceClose on all sockets', () => {
        const payload: StoreWorkerPayload = {
          op: 'dev:forceClose',
        };

        handler.handle(payload);

        expect(mockExchange.getAllSockets).toHaveBeenCalled();
        expect(mockForceClose).toHaveBeenCalledTimes(2);
      });
    });

    describe('other ops', () => {
      it('should not call any methods for unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
        };

        handler.handle(payload);

        expect(mockExchange.getAllSockets).not.toHaveBeenCalled();
        expect(vi.mocked(howWebSocketJA).simulateClose).not.toHaveBeenCalled();
        expect(vi.mocked(howWebSocketSG).simulateClose).not.toHaveBeenCalled();
      });
    });

    describe('with null sockets', () => {
      it('should handle null sockets gracefully', () => {
        (mockExchange.getAllSockets as any).mockReturnValue([null, undefined]);

        const payload: StoreWorkerPayload = {
          op: 'dev:simulateWebSocketsReconnect',
        };

        // Should not throw
        expect(() => handler.handle(payload)).not.toThrow();
      });
    });

    describe('with empty sockets array', () => {
      it('should handle empty sockets array', () => {
        (mockExchange.getAllSockets as any).mockReturnValue([]);

        const payload: StoreWorkerPayload = {
          op: 'dev:forceOpen',
        };

        handler.handle(payload);

        expect(mockForceOpen).not.toHaveBeenCalled();
      });
    });
  });
});
