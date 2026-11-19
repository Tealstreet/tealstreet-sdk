import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';
import type { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { nativePosition } from '../../__tests__/cutover/nativePosition';
import { applyReferenceFixture } from '../../__tests__/cutover/referenceFixture';
import { FrameStore } from '../../store/frameStore';
import { OrderStopTriggerType, PositionMode, PositionSide } from '../../types';
import { UiStateHandler } from './UiStateHandler';

describe('UiStateHandler', () => {
  let handler: UiStateHandler;
  let mockExchange: Exchange;
  let mockStore: FrameStore;
  let mockFetchLeverageForSymbol: ReturnType<typeof vi.fn>;
  let mockSetActiveTickerSymbols: ReturnType<typeof vi.fn>;
  let mockGetActiveTickerSymbols: ReturnType<typeof vi.fn>;
  let mockCtx: Worker;
  let mockSettings: AppSettingsWorkerType;
  let mockMetadata: ExchangeMetadataWorkerType;

  beforeEach(() => {
    vi.clearAllMocks();

    mockStore = new FrameStore();

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
        mockFetchLeverageForSymbol.mockImplementation(async () => {
          applyReferenceFixture(mockStore, {
            fetchedLeverage: { BTCUSDT: { buyLeverage: 10, sellLeverage: 10 } },
            positions: mockStore.positions.map((row) => ({
              ...row,
              leverage: row.side === PositionSide.Short ? 10 : 10,
            })),
          });
          return { buyLeverage: 10, sellLeverage: 10 };
        });

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
          expect(mockStore.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 10, sellLeverage: 10 });
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
        applyReferenceFixture(mockStore, { fetchedLeverage: { BTCUSDT: { buyLeverage: 10, sellLeverage: 10 } } });

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).not.toHaveBeenCalled();
      });

      it('coalesces requests while the native read is pending', () => {
        mockFetchLeverageForSymbol.mockReturnValue(new Promise(() => {}));
        handler.handle({ op: 'ui:state', data: { activeMarginSymbol: 'BTCUSDT' } });

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        expect(mockFetchLeverageForSymbol).toHaveBeenCalledTimes(1);
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
        mockFetchLeverageForSymbol.mockImplementation(async () => {
          applyReferenceFixture(mockStore, {
            fetchedLeverage: { BTCUSDT: { buyLeverage: 25, sellLeverage: 25 } },
            positions: mockStore.positions.map((row) => ({
              ...row,
              leverage: row.side === PositionSide.Short ? 25 : 25,
            })),
          });
          return { buyLeverage: 25, sellLeverage: 25 };
        });
        applyReferenceFixture(mockStore, {
          positions: [
            nativePosition({
              id: 'pos-1',
              symbol: 'BTCUSDT',
              leverage: 10,
              positionMode: PositionMode.OneWay,
              side: PositionSide.Long,
            }),
          ],
        });

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.positions[0]?.leverage).not.toBe(10);
        });

        // Check the update was called with correct leverage
        expect(mockStore.positions[0].leverage).toBe(25);
      });

      it('should use sellLeverage for short positions in hedge mode', async () => {
        mockFetchLeverageForSymbol.mockImplementation(async () => {
          applyReferenceFixture(mockStore, {
            fetchedLeverage: { BTCUSDT: { buyLeverage: 20, sellLeverage: 15 } },
            positions: mockStore.positions.map((row) => ({
              ...row,
              leverage: row.side === PositionSide.Short ? 15 : 20,
            })),
          });
          return { buyLeverage: 20, sellLeverage: 15 };
        });
        applyReferenceFixture(mockStore, {
          positions: [
            nativePosition({
              id: 'pos-1',
              symbol: 'BTCUSDT',
              leverage: 10,
              positionMode: PositionMode.Hedged,
              side: PositionSide.Short,
            }),
          ],
        });

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockStore.positions[0]?.leverage).not.toBe(10);
        });

        expect(mockStore.positions[0].leverage).toBe(15); // sellLeverage
      });

      it('should handle fetch failure gracefully', async () => {
        mockFetchLeverageForSymbol.mockResolvedValue(null);

        const payload: StoreWorkerPayload = {
          op: 'ui:state',
          data: { activeMarginSymbol: 'BTCUSDT' },
        };

        handler.handle(payload);

        await vi.waitFor(() => {
          expect(mockFetchLeverageForSymbol).toHaveBeenCalledExactlyOnceWith('BTCUSDT');
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
          expect(mockFetchLeverageForSymbol).toHaveBeenCalledExactlyOnceWith('BTCUSDT');
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
