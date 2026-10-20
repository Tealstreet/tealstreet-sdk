import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType, PositionMode, PositionSide } from '../../types';
import { UiStateHandler } from './UiStateHandler';

describe('UiStateHandler', () => {
  let handler: UiStateHandler;
  let mockExchange: Exchange;
  let mockStore: any;
  let mockFetchLeverageForSymbol: ReturnType<typeof vi.fn>;
  let mockSetActiveTickerSymbols: ReturnType<typeof vi.fn>;
  let mockGetActiveTickerSymbols: ReturnType<typeof vi.fn>;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;

  beforeEach(() => {
    vi.clearAllMocks();

    mockStore = {
      fetchedLeverage: {},
      positions: [],
      update: vi.fn((changes) => Object.assign(mockStore, changes)),
      updatePositions: vi.fn(),
    };

    mockFetchLeverageForSymbol = vi.fn();
    mockSetActiveTickerSymbols = vi.fn();
    mockGetActiveTickerSymbols = vi.fn().mockReturnValue([]);

    mockExchange = {
      name: 'bybit',
      store: mockStore,
      fetchLeverageForSymbol: mockFetchLeverageForSymbol,
      getActiveTickerSymbols: mockGetActiveTickerSymbols,
      setActiveTickerSymbols: mockSetActiveTickerSymbols,
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
      fetchLeverageOnMarginSymbolChange: true,
    };

    handler = new UiStateHandler(
      'test-account',
      'bybit' as ExchangeName,
      mockExchange,
      mockCtx,
      mockSettings,
      mockMetadata,
    );
  });

  describe('handle', () => {
    describe('ui:state', () => {
      it('should fetch leverage for new activeMarginSymbol', async () => {
        mockFetchLeverageForSymbol.mockResolvedValue({ buyLeverage: 10, sellLeverage: 10 });

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockSetActiveTickerSymbols).not.toHaveBeenCalled();

        // Should call fetchLeverageForSymbol
        expect(mockFetchLeverageForSymbol).toHaveBeenCalledWith('BTCUSDT');

        // Wait for async completion
        await vi.waitFor(() => {
          expect(mockStore.update).toHaveBeenCalled();
        });
      });

      it('should pass activeSymbols to exchange ticker symbol tracker', () => {
        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeSymbols: ['BTCUSDT', 'ETHUSDT', '', 123] },
        };

        handler.handle(payload);

        expect(mockSetActiveTickerSymbols).toHaveBeenCalledWith(['BTCUSDT', 'ETHUSDT']);
      });

      it('should merge activeSymbols with existing tracked symbols', () => {
        mockGetActiveTickerSymbols.mockReturnValue(['BTCUSDT', 'SOLUSDT']);
        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeSymbols: ['BTCUSDT', 'ETHUSDT'] },
        };

        handler.handle(payload);

        expect(mockSetActiveTickerSymbols).toHaveBeenCalledWith(['BTCUSDT', 'SOLUSDT', 'ETHUSDT']);
      });

      it('should not fetch if fetchLeverageOnMarginSymbolChange is false', () => {
        mockMetadata.fetchLeverageOnMarginSymbolChange = false;
        handler = new UiStateHandler(
          'test-account',
          'bybit' as ExchangeName,
          mockExchange,
          mockCtx,
          mockSettings,
          mockMetadata,
        );

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });

      it('should not fetch if leverage already exists for symbol', () => {
        mockStore.fetchedLeverage = {
          BTCUSDT: { buyLeverage: 10, sellLeverage: 10 },
        };

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });

      it('should not fetch if currently fetching for symbol', () => {
        mockStore.fetchedLeverage = {
          BTCUSDT: { buyLeverage: 0, sellLeverage: 0, isFetching: true },
        };

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });

      it('should not fetch if activeMarginSymbol is empty', () => {
        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: '' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });

      it('should update positions with fetched leverage', async () => {
        mockFetchLeverageForSymbol.mockResolvedValue({ buyLeverage: 25, sellLeverage: 25 });
        mockStore.positions = [
          {
            id: 'pos-1',
            symbol: 'BTCUSDT',
            leverage: 10,
            positionMode: PositionMode.OneWay,
            side: PositionSide.Long,
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.updatePositions).toHaveBeenCalled();
        });

        // Check the update was called with correct leverage
        const updateCall = mockStore.updatePositions.mock.calls[0][0];
        expect(updateCall[0][1].leverage).toBe(25);
      });

      it('should use sellLeverage for short positions in hedge mode', async () => {
        mockFetchLeverageForSymbol.mockResolvedValue({ buyLeverage: 20, sellLeverage: 15 });
        mockStore.positions = [
          {
            id: 'pos-1',
            symbol: 'BTCUSDT',
            leverage: 10,
            positionMode: PositionMode.Hedged,
            side: PositionSide.Short,
          },
        ];

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.updatePositions).toHaveBeenCalled();
        });

        const updateCall = mockStore.updatePositions.mock.calls[0][0];
        expect(updateCall[0][1].leverage).toBe(15); // sellLeverage
      });

      it('should handle fetch failure gracefully', async () => {
        mockFetchLeverageForSymbol.mockResolvedValue(null);

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          // Should have updated twice: once to mark fetching, once to remove marker
          expect(mockStore.update).toHaveBeenCalledTimes(2);
        });
        expect(mockStore.fetchedLeverage).not.toHaveProperty('BTCUSDT');
      });

      it('should handle fetch error gracefully', async () => {
        mockFetchLeverageForSymbol.mockRejectedValue(new Error('API error'));

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          // Should have updated twice: once to mark fetching, once to remove marker on error
          expect(mockStore.update).toHaveBeenCalledTimes(2);
        });
        expect(mockStore.fetchedLeverage).not.toHaveProperty('BTCUSDT');
      });
    });

    describe('other ops', () => {
      it('should not process unknown ops', () => {
        const payload: StoreWorkerPayload = {
          op: 'some-other-op' as any,
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });
    });
  });
});
