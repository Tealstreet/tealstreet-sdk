import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

/**
 * The refusal for an exchange name this build does not construct, including
 * retired venues still saved on user accounts (e.g. `bitmex`). It is typed and
 * sent nothing, so a caller fails that one account and keeps the others.
 */
export const unsupportedVenueError = (message: string): ExchangeError =>
  new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
