import invert from 'lodash/invert';

import { ExchangeName } from '../types';
import { convertSymbol } from '../utils/symbolConverter';
import { CrossTradeConfig, CrossTradeCopier } from './types';

export const getMasterSymbolFromCopier = ({
  config,
  copier,
  masterExchangeName,
  copierExchangeName,
  copierSymbol,
}: {
  config: CrossTradeConfig;
  copier: CrossTradeCopier;
  masterExchangeName?: ExchangeName;
  copierExchangeName: ExchangeName;
  copierSymbol: string;
}) => {
  // need to find master symbol from copier to check against
  const reversedSymbolMap = invert(copier.symbolMappings || {});
  return (
    // fixed master symbol
    config.masterSymbol ||
    // else user configured symbol
    reversedSymbolMap[copierSymbol] ||
    // else convert
    (masterExchangeName ? convertSymbol(copierSymbol, copierExchangeName, masterExchangeName) : copierSymbol)
  );
};
