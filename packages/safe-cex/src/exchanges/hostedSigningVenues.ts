import type { ExchangeName, ExchangeOptions } from '../types';

/**
 * A hosted account: no local secret, signed by the browser through `remoteSigner`.
 * Paradex and Polymarket keep their signing key in `key`, and Lighter runs read-only
 * without one, as TS does, so none of them is keyless here.
 */
export const isKeylessRemoteSigned = (
  name: ExchangeName,
  options: Partial<Pick<ExchangeOptions, 'secret' | 'remoteSigner' | 'auth'>>,
) =>
  Boolean(options.remoteSigner) &&
  !options.secret &&
  !(name === 'asterdex' && options.auth?.privateKey) &&
  !['lighter', 'lighter-robinhood', 'paradex', 'polymarket'].includes(name);
