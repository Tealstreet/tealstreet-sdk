import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { resolveAccountTransport } from './transportResolution';

describe('a stale tunnel flag', () => {
  it.each([
    [true, true],
    [false, false],
  ])('Lighter ignores it: the relay refuses its URL, so the account never starts (proxy %s)', (proxy, expected) => {
    expect(
      resolveAccountTransport({ tealstreetProxy: proxy, tealstreetTunnel: true } as any, exchangeMetadata.lighter),
    ).toMatchObject({ tealstreetProxy: expected, tealstreetTunnel: false });
  });

  it('another venue keeps it: its row may depend on that egress', () => {
    expect(resolveAccountTransport({ tealstreetTunnel: true } as any, exchangeMetadata.okx).tealstreetTunnel).toBe(true);
  });
});
