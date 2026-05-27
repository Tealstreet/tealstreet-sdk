import { ExchangeName } from '../types';
import { convertSymbol } from '../utils/symbolConverter';
import { CrossTradeCopier } from './types';

export const getCopierSymbolFromMaster = ({
  copier,
  copierExchangeName,
  masterExchangeName,
  masterSymbol,
}: {
  copier: CrossTradeCopier;
  copierExchangeName?: ExchangeName;
  masterExchangeName: ExchangeName;
  masterSymbol: string;
}) => {
  return (
    // user preferred symbol
    copier.symbolMappings?.[masterSymbol] ||
    // else convert from master to copier
    (copierExchangeName ? convertSymbol(masterSymbol, masterExchangeName, copierExchangeName) : masterSymbol)
  );
};
