import { Exchange } from '../../exchanges/base';
import { ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';

/**
 * Base class for post-processors that need the live Exchange instance.
 * These can only run where the exchange is instantiated (browser worker or server).
 */
export abstract class ExchangeDependentPostprocessor {
  protected _accountName: string;
  protected _exchangeName: ExchangeName;
  protected _options: ExchangeOptions;
  protected _exchange: Exchange;

  constructor(accountName: string, exchangeName: ExchangeName, options: ExchangeOptions, exchange: Exchange) {
    this._accountName = accountName;
    this._exchangeName = exchangeName;
    this._options = options;
    this._exchange = exchange;
  }

  abstract process(store: Writable<StoreData>): void;
}
