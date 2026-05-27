import { ExchangeName, ExchangeOptions, StoreData, Writable } from '../../types';

/**
 * Base class for post-processors that only need account info, options, and store data.
 * These can run anywhere — browser worker, server, main thread.
 */
export abstract class BasePostprocessor {
  protected _accountName: string;
  protected _exchangeName: ExchangeName;
  protected _options: ExchangeOptions;

  constructor(accountName: string, exchangeName: ExchangeName, options: ExchangeOptions) {
    this._accountName = accountName;
    this._exchangeName = exchangeName;
    this._options = options;
  }

  abstract process(store: Writable<StoreData>): void;
}
