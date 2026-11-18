import { describe, expect, it } from 'vitest';

import { resolveExchangeRouting } from '../resolveExchangeRouting';

// ApeX's private login refuses an IP-bound key dialled from a browser (live, 2026-11-02),
// so mainnet goes through the relay; the relay carries no testnet upstream.
describe('apexomni private socket', () => {
  it('goes through the relay to the mainnet private stream', () => {
    const ws = resolveExchangeRouting('apexomni', { tealstreetProxy: true }).wsPrivate;
    expect(ws.tunnelled).toBe(true);
    expect(decodeURIComponent(ws.host!)).toContain('wss://quote.omni.apex.exchange/realtime_private');
  });

  it('stays direct on testnet', () => {
    expect(resolveExchangeRouting('apexomni', { tealstreetProxy: true, testnet: true }).wsPrivate).toEqual({
      tunnelled: false,
    });
  });
});
