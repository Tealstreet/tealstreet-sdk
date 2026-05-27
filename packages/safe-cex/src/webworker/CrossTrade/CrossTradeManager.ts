import type { CrossTradeConfig, CrossTradeCopier } from '../../crossTrade/types';

import { getCopierSymbolFromMaster } from '../../crossTrade/getCopierSymbolFromMaster';
import { getCopierWeightRatio } from '../../crossTrade/getCopierWeightRatio';
import { getMasterSymbolFromCopier } from '../../crossTrade/getMasterSymbolFromCopier';
import { Exchange } from '../../exchanges/base';
import { OrderSide, PositionSide } from '../../types';
import { loop } from '../../utils/loop';
import { convertFromBaseSizeToNative } from '../../utils/sizing';
import { convertSymbol } from '../../utils/symbolConverter';
import { exchangeWorkers } from '../store.webworker';
import { crossTradePlaceOrder } from './crossTradePlaceOrder';

const debug = (...args: any[]) => console.log('[CrossTradeManager]', ...args);

export class CrossTradeManager {
  private _loop: {
    (): Promise<void>;
    isDisposed?: boolean;
  } | null = null;
  private _loopInterval = 3000;

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
    const masterPositions = masterStore.positions.filter((position) => !!position.contracts);

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
      (position) => !!position.contracts && (!relevantPositionSymbol || position.symbol === relevantPositionSymbol),
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
      if (!referencePrice) return;

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

      // close the copier position if not open in master
      if (!foundMasterPosition) {
        // no master position, close copier position
        return crossTradePlaceOrder(
          copier,
          copierMarket,
          // dont use reference price, we dont want to slip to position entry...
          orderPrice,
          copierAccount,
          copierPosition.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
          Math.abs(copierPosition.extra?.contractsNative || copierPosition.contracts),
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

        // contracts always in base
        let masterSizeInCopierBase = Math.abs(foundMasterPosition.contracts);

        // need to convert to copier base
        if (masterMarket.base !== copierMarket.base) {
          if (!masterTicker?.last || !copierTicker?.last) {
            debug('No ticker for', foundMasterPosition.symbol, copierPosition.symbol);
            return;
          }

          const masterSizeInUsd = masterTicker.last * masterSizeInCopierBase;
          masterSizeInCopierBase = masterSizeInUsd / copierTicker.last;
        }

        // contracts always in base
        const copierSizeInBase = Math.abs(copierPosition.contracts);

        if (!copierWeightRatio) return;

        const sizeDiff = masterSizeInCopierBase * copierWeightRatio - copierSizeInBase;
        const sizeDiffNative = convertFromBaseSizeToNative(sizeDiff, copierMarket, referencePrice);

        if (
          sizeDiffNative !== 0 &&
          //   greater than 5% diff
          Math.abs(sizeDiff / copierPosition.contracts) > 0.05
        ) {
          crossTradePlaceOrder(
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

            Math.abs(sizeDiffNative),
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
      (position) => !!position.contracts && (!config.masterSymbol || position.symbol === config.masterSymbol),
    );

    const copierPositions = copierExchange.store.positions.filter((position) => !!position.contracts);

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

      let referencePrice = copierTicker?.last;
      // dont use reference price, we dont want to slip to position entry...
      let orderPrice = copierTicker?.last;

      if (copierMarket.base === masterMarket.base) {
        referencePrice = referencePrice || masterTicker?.last || masterPosition.entryPrice;
        orderPrice = orderPrice || masterTicker?.last;
      }

      if (!referencePrice) return;

      // find the copier position
      const foundCopierPosition = copierPositions.find(
        (copierPosition) =>
          copierPosition.symbol === copierSymbol &&
          ((!isHedged && copierPosition.side === masterPosition.side) ||
            (isHedged && copierPosition.side !== masterPosition.side)),
      );

      if (!foundCopierPosition) {
        // contracts always in base
        let masterSizeInCopierBase = Math.abs(masterPosition.contracts);

        // need to convert to copier base
        if (masterMarket.base !== copierMarket.base) {
          if (!masterTicker?.last || !copierTicker?.last) {
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
        );

        const nonHedgeModeSide = masterPosition.side === PositionSide.Long ? OrderSide.Buy : OrderSide.Sell;

        // flip if hedge mode
        const orderSide = isHedged
          ? nonHedgeModeSide === OrderSide.Buy
            ? OrderSide.Sell
            : OrderSide.Buy
          : nonHedgeModeSide;

        return crossTradePlaceOrder(
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

        const masterExchange = exchangeWorkers[masterAccount]?.exchange;
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

            const copierExchange = exchangeWorkers[copier.account]?.exchange;
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
        console.error('Error in cross trade manager', e);
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
  }
}

export const crossTradeManagerInstance = new CrossTradeManager();
