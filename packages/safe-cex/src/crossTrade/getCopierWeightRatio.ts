import { Exchange } from '../exchanges/base';
import { Market } from '../types';
import { isBtcLikeSymbol, isEquivalentCurrency } from '@tealstreet/utils/src/currency/currencyUtils';
import { getTotalBalanceFromBalance } from '../utils/getTotalBalanceFromBalance';
import { convertFromMarginToQuote, getMarginCurrency } from '../utils/sizing';
import { CrossTradeConfig, CrossTradeCopier } from './types';

export const getCopierWeightRatio = (
  config: CrossTradeConfig,
  copier: CrossTradeCopier,
  masterExchange: Exchange,
  copierExchange: Exchange,
  masterMarket: Market,
  copierMarket: Market,
) => {
  const masterWeight = config.masterWeight || 1;

  let copierWeightRatio = copier.weight / masterWeight;
  if (copier.weightMode === 'Balance') {
    const copierStore = copierExchange.store;
    const masterStore = masterExchange.store;

    const copierAvailableMargin = getTotalBalanceFromBalance(copierStore.balance, copierMarket);
    const masterAvailableMargin = getTotalBalanceFromBalance(masterStore.balance, masterMarket);

    if (!copierAvailableMargin || !masterAvailableMargin) return 0;

    const copierMarginCurrency = getMarginCurrency(copierMarket);
    const masterMarginCurrency = getMarginCurrency(masterMarket);
    if (isEquivalentCurrency(copierMarginCurrency, masterMarginCurrency)) {
      copierWeightRatio *= copierAvailableMargin / masterAvailableMargin;
    } else {
      const copierBalanceInQuote = convertFromMarginToQuote(
        copierAvailableMargin,
        copierMarket,
        copierExchange.store?.tickersBySymbol[copierMarket.symbol]?.last,
        copierMarket.info?.isQuanto
          ? copierStore.tickers.find((ticker) => isBtcLikeSymbol(ticker.symbol))?.last
          : undefined,
      );
      const masterBalanceInQuote = convertFromMarginToQuote(
        masterAvailableMargin,
        masterMarket,
        masterExchange.store?.tickersBySymbol[masterMarket.symbol]?.last,
        masterMarket.info?.isQuanto
          ? masterStore.tickers.find((ticker) => isBtcLikeSymbol(ticker.symbol))?.last
          : undefined,
      );
      copierWeightRatio *= copierBalanceInQuote / masterBalanceInQuote;
    }
  }
  return Number.isFinite(copierWeightRatio) ? Math.abs(copierWeightRatio) : 0;
};
