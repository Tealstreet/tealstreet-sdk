import type { ExchangeName } from '../types';
import type { BaseExchange } from './base';

import { exchangeMetadata } from '../exchangeMetadata';
import { assertBetaVenue, BETA_VENUES } from './rust/betaVenues';

type ExchangeLoader = () => Promise<typeof BaseExchange>;

/** Known OUT names retain a typed refusal for saved accounts, without loading a venue. */
export const createRustRegistry = (load: ExchangeLoader): Record<ExchangeName, ExchangeLoader> =>
  Object.fromEntries(
    Object.keys(exchangeMetadata).map((name) => [
      name,
      async () => {
        assertBetaVenue(name);
        return load();
      },
    ]),
  ) as Record<ExchangeName, ExchangeLoader>;

export const getRegisteredExchangeNames = (): ExchangeName[] => [...BETA_VENUES];
