import { describe, it, expect } from 'vitest';
import type { AppSettingsWorkerType } from './AppSettingsWorkerType';
import type { ExchangeMetadataWorkerType } from './ExchangeMetadataWorkerType';
import { OrderStopTriggerType } from '../types';

describe('Worker Types', () => {
  describe('AppSettingsWorkerType', () => {
    it('should allow valid settings structure', () => {
      const settings: AppSettingsWorkerType = {
        placeOrder: {
          tpTriggerType: OrderStopTriggerType.MarkPrice,
          slTriggerType: OrderStopTriggerType.LastPrice,
        },
      };

      expect(settings.placeOrder.tpTriggerType).toBe(OrderStopTriggerType.MarkPrice);
      expect(settings.placeOrder.slTriggerType).toBe(OrderStopTriggerType.LastPrice);
    });

    it('should accept IndexPrice trigger type', () => {
      const settings: AppSettingsWorkerType = {
        placeOrder: {
          tpTriggerType: OrderStopTriggerType.IndexPrice,
          slTriggerType: OrderStopTriggerType.IndexPrice,
        },
      };

      expect(settings.placeOrder.tpTriggerType).toBe(OrderStopTriggerType.IndexPrice);
    });
  });

  describe('ExchangeMetadataWorkerType', () => {
    it('should allow minimal metadata with required fields', () => {
      const metadata: ExchangeMetadataWorkerType = {
        supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice],
        privateProxyIps: [],
        defaultTPTriggerType: undefined,
        defaultSLTriggerType: undefined,
      };

      expect(metadata.supportedTPSLTriggerTypes).toHaveLength(1);
      expect(metadata.privateProxyIps).toEqual([]);
    });

    it('should allow full metadata with all fields', () => {
      const metadata: ExchangeMetadataWorkerType = {
        supportedTPSLTriggerTypes: [
          OrderStopTriggerType.LastPrice,
          OrderStopTriggerType.MarkPrice,
          OrderStopTriggerType.IndexPrice,
        ],
        privateProxyIps: ['192.168.1.1', '192.168.1.2'],
        defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
        defaultSLTriggerType: OrderStopTriggerType.LastPrice,
        fetchLeverageOnMarginSymbolChange: true,
      };

      expect(metadata.supportedTPSLTriggerTypes).toHaveLength(3);
      expect(metadata.privateProxyIps).toHaveLength(2);
      expect(metadata.defaultTPTriggerType).toBe(OrderStopTriggerType.MarkPrice);
      expect(metadata.fetchLeverageOnMarginSymbolChange).toBe(true);
    });

    it('should allow empty trigger types array', () => {
      const metadata: ExchangeMetadataWorkerType = {
        supportedTPSLTriggerTypes: [],
        privateProxyIps: [],
        defaultTPTriggerType: undefined,
        defaultSLTriggerType: undefined,
      };

      expect(metadata.supportedTPSLTriggerTypes).toEqual([]);
    });
  });
});
