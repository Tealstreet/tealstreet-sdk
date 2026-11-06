import type { Store } from '../../../store/store.interface';
import type { ExchangeName, ExchangeOptions } from '../../../types';
import { BitgetExchange } from './bitget.exchange';

/**
 * BitGet USDC Exchange
 *
 * Extends BitgetExchange to support USDC-margined futures contracts.
 * Uses usdc-futures (or susdc-futures for testnet) as the product type.
 */
export class BitgetUSDCExchange extends BitgetExchange {
  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
  }

  get apiProductType(): 'susdt-futures' | 'usdt-futures' {
    return this.options.testnet ? ('susdc-futures' as any) : ('usdc-futures' as any);
  }

  get apiMarginCoin(): 'USDT' | 'SUSDT' {
    return this.options.testnet ? ('SUSDC' as any) : ('USDC' as any);
  }
}
