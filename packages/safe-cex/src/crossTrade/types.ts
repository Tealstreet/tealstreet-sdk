export type CrossTradWeightMode = 'Balance' | 'Absolute';
export type CrossTradeExecution = 'LimitSlippage' | 'Market' | 'Chase';

export const defaultCrossTradeCopier = Object.freeze({
  enabled: false as boolean,
  account: undefined as string | undefined,
  weight: 1 as number,
  execution: 'LimitSlippage' as CrossTradeExecution,
  weightMode: 'Absolute' as CrossTradWeightMode,
  symbolMappings: {} as { [key: string]: string },
});

export const defaultCrossTradeConfig = Object.freeze({
  enabled: false as boolean,
  masterAccount: undefined as string | undefined,
  masterSymbol: undefined as string | undefined,
  masterWeight: 1 as number,
  copiers: [] as CrossTradeCopier[],
});

export type CrossTradeCopier = typeof defaultCrossTradeCopier;
export type CrossTradeConfig = typeof defaultCrossTradeConfig;