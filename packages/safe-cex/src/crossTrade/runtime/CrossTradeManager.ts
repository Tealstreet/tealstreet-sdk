import type { Exchange } from '../../exchanges/base';
import type { Market } from '../../types';
import type { CrossTradeConfig, CrossTradeCopier } from '../types';

import { OrderSide, PositionSide } from '../../types';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { loop } from '../../utils/loop';
import { nativePositionSize } from '../../utils/nativePositionSize';
import { convertFromBaseSizeToNative } from '../../utils/sizing';
import { convertSymbol } from '../../utils/symbolConverter';
import { getCopierSymbolFromMaster } from '../getCopierSymbolFromMaster';
import { getCopierWeightRatio } from '../getCopierWeightRatio';
import { getMasterSymbolFromCopier } from '../getMasterSymbolFromCopier';

const debug = (...args: unknown[]) => diagnosticConsole.log('[CrossTradeManager]', ...args);

// Keep canonical quantities unrounded until the order's lot-step conversion.
const baseSizeFromNative = (native: number, market: Market, price: number): number | null => {
  if (market.isInverse && (!Number.isFinite(price) || price <= 0)) return null;
  const base = (native * (market.contractSize || 1)) / (market.isInverse ? price : 1);
  return Number.isFinite(base) && (native === 0 || base > 0) ? base : null;
};

const FAILURE_THRESHOLD = 2;
const FAILURE_MUTE_MS = 30_000;
/**
 * The engine refuses a symbol whose earlier placement's outcome is unknown until a fresh
 * order read resolves it. Retrying it every loop only repeats the refusal: hold instead.
 */
const UNCERTAIN_HOLD_MS = 15_000;
const UNCERTAIN_HOLD_MAX_MS = 5 * 60_000;
const isUncertainRefusal = (error: unknown) =>
  error instanceof Error && error.message.includes('Placement outcome is uncertain');

export type CrossTradePlaceOrderFn = (
  copier: CrossTradeCopier,
  market: Market,
  referencePrice: number | undefined,
  account: string,
  side: OrderSide,
  amount: number,
  reduceOnly: boolean,
) => Promise<unknown>;

export type CrossTradeGetExchangeFn = (account: string) => Exchange | undefined;

export interface CrossTradeManagerDeps {
  getExchange: CrossTradeGetExchangeFn;
  placeOrder: CrossTradePlaceOrderFn;
}

export class CrossTradeManager {
  private _loop: {
    (): Promise<void>;
    isDisposed?: boolean;
  } | null = null;
  private _loopInterval = 3000;
  private _failures = new Map<string, { failures: number; mutedUntil: number; holds?: number }>();
  private _getExchange: CrossTradeGetExchangeFn;
  private _placeOrder: CrossTradePlaceOrderFn;

  constructor(deps: CrossTradeManagerDeps) {
    this._getExchange = deps.getExchange;
    this._placeOrder = deps.placeOrder;
  }

  private _failureKey(account: string, symbol: string) {
    return `${account}:${symbol}`;
  }

  private _isMuted(account: string, symbol: string): boolean {
    const entry = this._failures.get(this._failureKey(account, symbol));
    return !!entry && entry.mutedUntil > Date.now();
  }

  private _recordFailure(account: string, symbol: string) {
    const key = this._failureKey(account, symbol);
    const entry = this._failures.get(key) ?? { failures: 0, mutedUntil: 0 };
    entry.failures += 1;
    if (entry.failures >= FAILURE_THRESHOLD) {
      entry.mutedUntil = Date.now() + FAILURE_MUTE_MS;
      debug('Muting', key, 'until', new Date(entry.mutedUntil).toISOString());
    }
    this._failures.set(key, entry);
  }

  private _recordUncertain(account: string, symbol: string) {
    const key = this._failureKey(account, symbol);
    const entry = this._failures.get(key) ?? { failures: 0, mutedUntil: 0 };
    entry.holds = (entry.holds ?? 0) + 1;
    const holdMs = Math.min(UNCERTAIN_HOLD_MS * 2 ** (entry.holds - 1), UNCERTAIN_HOLD_MAX_MS);
    entry.mutedUntil = Date.now() + holdMs;
    this._failures.set(key, entry);
    diagnosticConsole.warn('[CrossTrade] holding after an uncertain placement', { source: 'cross-trade', symbol, holdMs });
  }

  private _recordSuccess(account: string, symbol: string) {
    this._failures.delete(this._failureKey(account, symbol));
  }

  private _meetsMinAmount(market: Market, amount: number): boolean {
    const minAmount = market.limits?.amount?.min ?? 0;
    if (minAmount <= 0) return true;
    return Math.abs(amount) >= minAmount;
  }

  private async _safePlaceOrder(
    copier: CrossTradeCopier,
    market: Market,
    referencePrice: number | undefined,
    account: string,
    side: OrderSide,
    amount: number,
    reduceOnly: boolean,
  ) {
    const symbol = market.symbol;
    if (this._isMuted(account, symbol)) {
      debug('Skipping muted', account, symbol);
      return;
    }
    // Allow reduceOnly closes through the min-amount gate so a residual position
    // below the exchange minimum still has a chance to be flattened.
    if (!reduceOnly && !this._meetsMinAmount(market, amount)) {
      debug('Skipping below-min-amount order', account, symbol, amount, 'min', market.limits?.amount?.min);
      return;
    }
    // A breadcrumb per attempt: these orders have no user action behind them in the page.
    diagnosticConsole.info('[CrossTrade] placing', {
      source: 'cross-trade',
      exchange: this._getExchange(account)?.name,
      symbol,
      side,
      amount,
      reduceOnly,
      execution: copier.execution,
    });
    try {
      const result = await this._placeOrder(copier, market, referencePrice, account, side, amount, reduceOnly);
      this._recordSuccess(account, symbol);
      return result;
    } catch (error) {
      if (isUncertainRefusal(error)) this._recordUncertain(account, symbol);
      else this._recordFailure(account, symbol);
    }
  }

  /**
   * Check that all copier positions are correct in the master. Reverse of _auditMasterPositions
   */
  private async _auditCopierPositions(
    config: CrossTradeConfig,
    copier: CrossTradeCopier,
    masterExchange: Exchange,
    copierExchange: Exchange,
  ) {
    const copierAccount = copier.account;
    if (!copierAccount) return;

    // can be used in future, currently always 1
    const masterWeight = config.masterWeight || 1;
    const isHedged = copier.weight / masterWeight < 0;

    const copierStore = copierExchange.store;
    const copierExchangeName = copierExchange.name;

    const masterStore = masterExchange.store;
    const masterExchangeName = masterExchange.name;
    const masterPositions = masterStore.positions;

    let relevantPositionSymbol = undefined;

    if (config.masterSymbol) {
      relevantPositionSymbol =
        copier.symbolMappings?.[config.masterSymbol] ||
        (copierExchangeName
          ? convertSymbol(config.masterSymbol, masterExchangeName, copierExchangeName)
          : config.masterSymbol);
    }

    // check all positions if not master symbol, else only position with symbol matching for copier
    const copierPositions = copierExchange.store.positions.filter(
      (position) => !relevantPositionSymbol || position.symbol === relevantPositionSymbol,
    );

    // check copier positions are valid
    const requests = copierPositions.map((copierPosition) => {
      const copierMarket = copierStore.markets.find((market) => market.symbol === copierPosition.symbol);
      const copierTicker = copierStore.tickers.find((ticker) => ticker.symbol === copierPosition.symbol);

      const masterSymbol = getMasterSymbolFromCopier({
        config,
        copier,
        masterExchangeName,
        copierExchangeName,
        copierSymbol: copierPosition.symbol,
      });

      const masterMarket = masterStore.markets.find((market) => market.symbol === masterSymbol);
      const masterTicker = masterStore.tickers.find((ticker) => ticker.symbol === masterSymbol);

      if (!copierMarket || !masterMarket) {
        debug('Could not find market for', copierPosition.symbol, masterSymbol);
        return;
      }

      let orderPrice = copierTicker?.last;
      let referencePrice = copierTicker?.last;

      // dont use reference price, we dont want to slip to position entry...
      if (copierMarket.base === masterMarket.base) {
        referencePrice = referencePrice || masterTicker?.last || copierPosition.entryPrice;
        orderPrice = orderPrice || masterTicker?.last;
      }

      // need reference price
      if (!referencePrice || !Number.isFinite(referencePrice) || referencePrice <= 0) return;

      const copierNative = nativePositionSize(copierPosition, copierMarket);
      if (copierNative === null) {
        debug('Skipping reconciliation with unknown copier native size', copierPosition.symbol);
        return;
      }
      if (copierNative === 0) return;

      // associated position in master
      let foundMasterPosition = undefined;
      const selectedCopierSymbolForMaster = copier.symbolMappings?.[masterSymbol];
      if (!selectedCopierSymbolForMaster || selectedCopierSymbolForMaster === copierPosition.symbol) {
        foundMasterPosition = masterPositions.find(
          (masterPosition) =>
            masterPosition.symbol === masterSymbol &&
            ((!isHedged && masterPosition.side === copierPosition.side) ||
              (isHedged && masterPosition.side !== copierPosition.side)),
        );
      }

      const masterNative = foundMasterPosition ? nativePositionSize(foundMasterPosition, masterMarket) : 0;
      if (masterNative === null) {
        debug('Skipping reconciliation with unknown master native size', masterSymbol);
        return;
      }

      // close the copier position if not open in master
      if (!foundMasterPosition || masterNative === 0) {
        // no master position, close copier position
        return this._safePlaceOrder(
          copier,
          copierMarket,
          // dont use reference price, we dont want to slip to position entry...
          orderPrice,
          copierAccount,
          copierPosition.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
          copierNative,
          true,
        );
      } else {
        // else we need to audit size is correct

        const copierWeightRatio = getCopierWeightRatio(
          config,
          copier,
          masterExchange,
          copierExchange,
          masterMarket,
          copierMarket,
        );

        let masterSizeInCopierBase = baseSizeFromNative(
          masterNative,
          masterMarket,
          masterTicker?.last || referencePrice,
        );
        const copierSizeInBase = baseSizeFromNative(copierNative, copierMarket, referencePrice);
        if (masterSizeInCopierBase === null || copierSizeInBase === null) return;

        // need to convert to copier base
        if (masterMarket.base !== copierMarket.base) {
          if (
            !masterTicker?.last ||
            !copierTicker?.last ||
            !Number.isFinite(masterTicker.last) ||
            !Number.isFinite(copierTicker.last) ||
            masterTicker.last <= 0 ||
            copierTicker.last <= 0
          ) {
            debug('No ticker for', foundMasterPosition.symbol, copierPosition.symbol);
            return;
          }

          const masterSizeInUsd = masterTicker.last * masterSizeInCopierBase;
          masterSizeInCopierBase = masterSizeInUsd / copierTicker.last;
        }

        if (!copierWeightRatio) return;

        const sizeDiff = masterSizeInCopierBase * copierWeightRatio - copierSizeInBase;
        const sizeDiffNative = convertFromBaseSizeToNative(sizeDiff, copierMarket, referencePrice, copierExchangeName);

        if (
          Number.isFinite(sizeDiffNative) &&
          sizeDiffNative !== 0 &&
          //   greater than 5% diff
          Math.abs(sizeDiff / copierSizeInBase) > 0.05
        ) {
          return this._safePlaceOrder(
            copier,
            copierMarket,
            // dont use reference price, we dont want to slip to position entry...
            orderPrice,
            copierAccount,
            copierPosition.side === PositionSide.Long
              ? sizeDiffNative > 0
                ? OrderSide.Buy
                : OrderSide.Sell
              : sizeDiffNative > 0
                ? OrderSide.Sell
                : OrderSide.Buy,

            sizeDiffNative < 0 ? Math.min(-sizeDiffNative, copierNative) : sizeDiffNative,
            sizeDiffNative < 0,
          );
        }
      }
    });
    return await Promise.all(requests);
  }

  /**
   * Check that all master positions are correct in the copier. Reverse of _auditCopierPositions
   */
  private async _auditMasterPositions(
    config: CrossTradeConfig,
    copier: CrossTradeCopier,
    masterExchange: Exchange,
    copierExchange: Exchange,
  ) {
    const copierAccount = copier.account;
    if (!copierAccount) return;

    // can be used in future, currently always 1
    const masterWeight = config.masterWeight || 1;
    const isHedged = copier.weight / masterWeight < 0;

    const copierStore = copierExchange.store;
    const copierExchangeName = copierExchange.name;
    const masterExchangeName = masterExchange.name;
    const masterStore = masterExchange.store;

    // check all positions if not master symbol, else only that position
    const masterPositions = masterStore.positions.filter(
      (position) => !config.masterSymbol || position.symbol === config.masterSymbol,
    );

    const copierPositions = copierExchange.store.positions;

    // check positions in master are correct in copier
    const requests = masterPositions.map((masterPosition) => {
      const masterMarket = masterStore.markets.find((market) => market.symbol === masterPosition.symbol);
      const masterTicker = masterStore.tickers.find((ticker) => ticker.symbol === masterPosition.symbol);

      const copierSymbol = getCopierSymbolFromMaster({
        copier,
        copierExchangeName,
        masterExchangeName,
        masterSymbol: masterPosition.symbol,
      });

      const copierMarket = copierStore.markets.find((market) => market.symbol === copierSymbol);
      const copierTicker = copierStore.tickers.find((ticker) => ticker.symbol === copierSymbol);

      if (!copierMarket || !masterMarket) {
        debug('Could not find market for', masterPosition.symbol, copierSymbol);
        return;
      }

      const masterNative = nativePositionSize(masterPosition, masterMarket);
      if (masterNative === null || masterNative === 0) return;

      let referencePrice = copierTicker?.last;
      // dont use reference price, we dont want to slip to position entry...
      let orderPrice = copierTicker?.last;

      if (copierMarket.base === masterMarket.base) {
        referencePrice = referencePrice || masterTicker?.last || masterPosition.entryPrice;
        orderPrice = orderPrice || masterTicker?.last;
      }

      if (!referencePrice || !Number.isFinite(referencePrice) || referencePrice <= 0) return;

      const copierSizes = copierPositions
        .filter((position) => position.symbol === copierSymbol)
        .map((position) => ({ position, native: nativePositionSize(position, copierMarket) }));
      // An opposite-side entry can reduce a position too, even without reduceOnly.
      if (copierSizes.some(({ native }) => native === null)) return;
      const hasCopierPosition = copierSizes.some(
        ({ position, native }) =>
          native !== null &&
          native > 0 &&
          ((!isHedged && position.side === masterPosition.side) || (isHedged && position.side !== masterPosition.side)),
      );

      if (!hasCopierPosition) {
        let masterSizeInCopierBase = baseSizeFromNative(
          masterNative,
          masterMarket,
          masterTicker?.last || referencePrice,
        );
        if (masterSizeInCopierBase === null) return;

        // need to convert to copier base
        if (masterMarket.base !== copierMarket.base) {
          if (
            !masterTicker?.last ||
            !copierTicker?.last ||
            !Number.isFinite(masterTicker.last) ||
            !Number.isFinite(copierTicker.last) ||
            masterTicker.last <= 0 ||
            copierTicker.last <= 0
          ) {
            debug('No ticker for', masterPosition.symbol, copierSymbol);
            return;
          }
          const masterSizeInUsd = masterTicker.last * masterSizeInCopierBase;
          masterSizeInCopierBase = masterSizeInUsd / copierTicker.last;
        } else {
          // if same units we can try other ways to get the master ticker
          referencePrice = referencePrice || masterTicker?.last || masterPosition.entryPrice;
          orderPrice = orderPrice || masterTicker?.last;
        }

        const copierWeightRatio = getCopierWeightRatio(
          config,
          copier,
          masterExchange,
          copierExchange,
          masterMarket,
          copierMarket,
        );

        if (!copierWeightRatio || !referencePrice) return;

        // in units for copier market
        const copierSizeNative = convertFromBaseSizeToNative(
          masterSizeInCopierBase * copierWeightRatio,
          copierMarket,
          referencePrice,
          copierExchangeName,
        );
        if (!Number.isFinite(copierSizeNative) || copierSizeNative <= 0) return;

        const nonHedgeModeSide = masterPosition.side === PositionSide.Long ? OrderSide.Buy : OrderSide.Sell;

        // flip if hedge mode
        const orderSide = isHedged
          ? nonHedgeModeSide === OrderSide.Buy
            ? OrderSide.Sell
            : OrderSide.Buy
          : nonHedgeModeSide;

        return this._safePlaceOrder(
          copier,
          copierMarket,
          orderPrice,
          copierAccount,
          orderSide,
          copierSizeNative,
          false,
        );
      }
    });
    return await Promise.all(requests);
  }

  /**
   * Start single loop to process all configs
   * @param crossTradeConfigs
   */
  start(crossTradeConfigs: CrossTradeConfig[]) {
    if (this._loop) {
      this._loop.isDisposed = true;
    }

    debug('Starting cross trade manager', crossTradeConfigs);
    const newLoop = async () => {
      if (!this._loop || newLoop.isDisposed) return;

      const processConfig = async (config: CrossTradeConfig) => {
        const { masterAccount, copiers } = config;

        if (!masterAccount) {
          debug('No master account for cross trade config', config);
          return;
        }

        const masterExchange = this._getExchange(masterAccount);
        if (!masterExchange?.store?.loaded?.positions) {
          debug('Master exchange not loaded', masterAccount);
          return;
        }

        await Promise.all(
          copiers.map(async (copier: CrossTradeCopier) => {
            if (!copier || !copier.enabled || !copier.account) {
              debug('Invalid copier', copier);
              return;
            }

            const copierExchange = this._getExchange(copier.account);
            if (!copierExchange) {
              debug('No copier exchange', copier.account);
              return;
            }

            if (!copierExchange?.store?.loaded?.positions) {
              debug('Copier exchange not loaded', copier.account);
              return;
            }

            // the actual meat
            await this._auditCopierPositions(config, copier, masterExchange, copierExchange);
            if (!this._loop || newLoop.isDisposed) return;
            await this._auditMasterPositions(config, copier, masterExchange, copierExchange);
          }),
        );
      };

      try {
        await Promise.all(crossTradeConfigs.map(processConfig));
      } catch (e) {
        diagnosticConsole.error('Error in cross trade manager', e);
      }

      if (!this._loop || newLoop.isDisposed) return;
      loop(this._loop, this._loopInterval);
    };

    newLoop.isDisposed = false;
    loop(newLoop, this._loopInterval);
    this._loop = newLoop;
  }

  stop() {
    if (this._loop) {
      debug('Stopping cross trade manager');
      this._loop.isDisposed = true;
    }
    this._failures.clear();
  }
}
