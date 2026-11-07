import type { ExchangeName, ExchangeOptions } from '../../types';

import { isLighterRobinhoodExchange } from './lighter.types';

export type LighterBuilderFeeMode = 'dynamic' | 'static';

// Placeholder values: replace with real Tealstreet integrator account indexes later.
export const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET = '158385';
export const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET =
  'REPLACE_WITH_LIGHTER_TESTNET_INTEGRATOR_ACCOUNT_INDEX';
export const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD = '47296';

// Lighter partner fee value unit: fee_value / 1e6 of trade size.
// 250 = 2.5 bps.
export const LIGHTER_INTEGRATOR_MAKER_FEE_VALUE = 0;
export const LIGHTER_INTEGRATOR_TAKER_FEE_VALUE = 250;

export type LighterBuilderFeeConfig = {
  integratorAccountIndex: bigint;
  integratorMakerFee: number;
  integratorTakerFee: number;
  mode: LighterBuilderFeeMode;
};

const resolveMode = (value: unknown): LighterBuilderFeeMode => {
  // Default to dynamic mode unless static is explicitly requested.
  return value === 'static' ? 'static' : 'dynamic';
};

const parsePositiveIntegerBigInt = (value: unknown): bigint | undefined => {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) {
    return undefined;
  }

  try {
    const parsed = BigInt(trimmed);
    if (parsed < 0n) {
      return undefined;
    }
    return parsed;
  } catch {
    return undefined;
  }
};

const getDefaultIntegratorAccountIndex = (
  options: ExchangeOptions,
  exchangeName?: ExchangeName,
): string | undefined => {
  if (isLighterRobinhoodExchange(exchangeName)) {
    return TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD || undefined;
  }

  return options.testnet
    ? TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
    : TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET;
};

export const resolveLighterBuilderFeeConfig = (
  options: ExchangeOptions,
  exchangeName?: ExchangeName,
): LighterBuilderFeeConfig | undefined => {
  const mode = resolveMode(options.extra?.lighterBuilderFeeMode);
  const configuredIntegratorIndex = options.extra?.lighterIntegratorAccountIndex;
  const defaultIntegratorIndex = getDefaultIntegratorAccountIndex(options, exchangeName);

  const integratorAccountIndex = parsePositiveIntegerBigInt(configuredIntegratorIndex ?? defaultIntegratorIndex);
  if (integratorAccountIndex === undefined) {
    return undefined;
  }

  return {
    integratorAccountIndex,
    integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
    integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
    mode,
  };
};
