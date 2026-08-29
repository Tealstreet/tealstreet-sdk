import type { ExchangeOptions, PlaceOrderOpts } from '../../types';

import { ethers } from 'ethers';

import { OrderTimeInForce, OrderType } from '../../types';

export const TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET = '0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca';
export const TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET = 'REPLACE_WITH_TESTNET_BUILDER_ADDRESS';

// Aster futures maxFeeRate cap = 0.001 (10 bps). Default to the current minimum policy: 1 bps.
export const DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER = '0.0001';
export const DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER = '0.0001';

export const ASTER_BUILDER_FEE_RATE_CAP = '0.001';

export type AsterBuilderFeeConfig = {
  builderAddress: string;
  feeRateMaker: string;
  feeRateTaker: string;
};

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';
const DECIMAL_REGEX = /^(?:0|[1-9]\d*)(?:\.\d+)?$/;
const DECIMAL_SCALE = 18;
const DECIMAL_SCALE_FACTOR = 10n ** BigInt(DECIMAL_SCALE);

const parseScaledDecimal = (value: string): bigint | undefined => {
  if (!DECIMAL_REGEX.test(value)) {
    return undefined;
  }

  const [integerPart, fractionalPart = ''] = value.split('.');
  if (fractionalPart.length > DECIMAL_SCALE) {
    return undefined;
  }

  return BigInt(integerPart) * DECIMAL_SCALE_FACTOR + BigInt(fractionalPart.padEnd(DECIMAL_SCALE, '0'));
};

const normalizeDecimalString = (value: string): string | undefined => {
  const trimmed = value.trim();
  const scaled = parseScaledDecimal(trimmed);
  if (scaled === undefined) {
    return undefined;
  }

  if (scaled === 0n) {
    return '0';
  }

  const [integerPart, fractionalPart = ''] = trimmed.split('.');
  const normalizedInteger = integerPart.replace(/^0+(?=\d)/, '') || '0';
  const normalizedFraction = fractionalPart.replace(/0+$/, '');
  return normalizedFraction ? `${normalizedInteger}.${normalizedFraction}` : normalizedInteger;
};

const assertFeeRate = (value: string, fieldName: string): string => {
  const normalized = normalizeDecimalString(value);
  if (normalized === undefined) {
    throw new Error(`Invalid Aster builder ${fieldName}: expected a decimal string`);
  }

  const scaled = parseScaledDecimal(normalized);
  const cap = parseScaledDecimal(ASTER_BUILDER_FEE_RATE_CAP);
  if (scaled === undefined || cap === undefined || scaled > cap) {
    throw new Error(`Invalid Aster builder ${fieldName}: maximum supported fee rate is ${ASTER_BUILDER_FEE_RATE_CAP}`);
  }

  return normalized;
};

const resolveBuilderAddress = (options: ExchangeOptions): string | undefined => {
  const raw =
    options.extra?.asterBuilderAddress ||
    (options.testnet ? TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET : TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET);
  const trimmed = raw.trim();
  if (!trimmed || trimmed.startsWith('REPLACE_WITH_')) {
    return undefined;
  }

  let checksummed: string;
  try {
    checksummed = ethers.getAddress(trimmed);
  } catch {
    throw new Error('Invalid Aster builder address: expected an EVM address');
  }

  if (checksummed === ZERO_ADDRESS) {
    throw new Error('Invalid Aster builder address: zero address is not allowed');
  }

  return checksummed;
};

export const resolveAsterBuilderFeeConfig = (options: ExchangeOptions): AsterBuilderFeeConfig | undefined => {
  const feeRateMaker = assertFeeRate(
    options.extra?.asterBuilderFeeRateMaker ?? DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER,
    'maker fee rate',
  );
  const feeRateTaker = assertFeeRate(
    options.extra?.asterBuilderFeeRateTaker ?? DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER,
    'taker fee rate',
  );

  if (feeRateMaker === '0' && feeRateTaker === '0') {
    return undefined;
  }

  const builderAddress = resolveBuilderAddress(options);
  if (!builderAddress) {
    return undefined;
  }

  return {
    builderAddress,
    feeRateMaker,
    feeRateTaker,
  };
};

export const isAsterMakerOnlyOrder = (
  opts: Pick<PlaceOrderOpts, 'timeInForce' | 'type'> & { readonly postOnly?: boolean },
  apiOrderType: string,
): boolean =>
  opts.type === OrderType.Limit &&
  (apiOrderType === 'LIMIT' || apiOrderType === 'STOP' || apiOrderType === 'TAKE_PROFIT') &&
  (opts.timeInForce === OrderTimeInForce.PostOnly || opts.postOnly === true);

export const getAsterBuilderFeeRateForOrder = (
  opts: Pick<PlaceOrderOpts, 'timeInForce' | 'type'> & { readonly postOnly?: boolean },
  apiOrderType: string,
  config: AsterBuilderFeeConfig,
): string | undefined => {
  const feeRate = isAsterMakerOnlyOrder(opts, apiOrderType) ? config.feeRateMaker : config.feeRateTaker;
  return feeRate === '0' ? undefined : feeRate;
};

export const getAsterBuilderApprovalFeeRate = (config: AsterBuilderFeeConfig): string => {
  const maker = parseScaledDecimal(config.feeRateMaker) ?? 0n;
  const taker = parseScaledDecimal(config.feeRateTaker) ?? 0n;
  return maker >= taker ? config.feeRateMaker : config.feeRateTaker;
};
