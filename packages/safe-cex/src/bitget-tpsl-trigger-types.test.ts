import { describe, expect, it } from 'vitest';

import { exchangeMetadata, getExchangeMetadata } from './exchangeMetadata';
import { ExchangeName, OrderStopTriggerType } from './types';

const selectable = [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice];

describe.each(['bitget', 'bitgetusdc', 'bitgetinverse'] as ExchangeName[])('%s TP/SL paths', (venue) => {
  const utaSelectable = venue === 'bitgetinverse' ? [] : selectable;
  it('offers no preset selector or invented default for Classic and unknown accounts', () => {
    expect(exchangeMetadata[venue].supportedTPSLTriggerTypes).toEqual([]);
    for (const bitgetAccountMode of [undefined, 'classic']) {
      const metadata = getExchangeMetadata(venue, { bitgetAccountMode, tpslOrderPath: 'entry' });
      expect(metadata.supportedTPSLTriggerTypes).toEqual([]);
      expect(metadata.defaultTPTriggerType).toBeUndefined();
      expect(metadata.defaultSLTriggerType).toBeUndefined();
    }
  });

  it('offers only the references its UTA entry route can honor', () => {
    const metadata = getExchangeMetadata(venue, { bitgetAccountMode: 'uta', tpslOrderPath: 'entry' });
    expect(metadata.supportedTPSLTriggerTypes).toEqual(utaSelectable);
    const defaultTrigger = venue === 'bitgetinverse' ? undefined : OrderStopTriggerType.MarkPrice;
    expect(metadata.defaultTPTriggerType).toBe(defaultTrigger);
    expect(metadata.defaultSLTriggerType).toBe(defaultTrigger);
  });

  it('offers Last and Mark for position protection in either account mode', () => {
    for (const bitgetAccountMode of [undefined, 'classic', 'uta']) {
      const metadata = getExchangeMetadata(venue, { bitgetAccountMode, tpslOrderPath: 'position' });
      expect(metadata.supportedTPSLTriggerTypes).toEqual(bitgetAccountMode === 'uta' ? utaSelectable : selectable);
      expect(metadata.supportedTPSLTriggerTypes).not.toContain(OrderStopTriggerType.IndexPrice);
    }
  });

  it('does not infer UTA from a truthy unrelated flag or mutate shared metadata', () => {
    expect(getExchangeMetadata(venue, { bitgetAccountMode: 'uta' }).supportedTPSLTriggerTypes).toEqual(utaSelectable);
    expect(getExchangeMetadata(venue, { isUnified: true }).supportedTPSLTriggerTypes).toEqual([]);
    expect(getExchangeMetadata(venue, { bitgetAccountMode: 'UTA' }).supportedTPSLTriggerTypes).toEqual([]);
    expect(getExchangeMetadata(venue).supportedTPSLTriggerTypes).toEqual([]);
  });
});
