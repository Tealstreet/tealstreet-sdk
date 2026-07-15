import { isNotNumber } from '@tealstreet/math/src/isNotNumber';

// Pre-computed powers of 10 for fast lookup (covers 0-20 decimal places)
const POW10_CACHE: number[] = new Array(21);
for (let i = 0; i <= 20; i++) {
  POW10_CACHE[i] = 10 ** i;
}

// Fast power of 10 with cache lookup
const pow10 = (n: number): number => {
  if (n >= 0 && n <= 20 && Number.isInteger(n)) {
    return POW10_CACHE[n];
  }
  return 10 ** n;
};

export const afterDecimal = (num: number | string) => {
  // Fast path for integers
  if (typeof num === 'number') {
    if (Number.isInteger(num)) return 0;
    // For non-integers, we still need string conversion for accuracy
  }

  const str = typeof num === 'number' ? String(num) : num?.toString?.();
  if (!str) return 2;

  // Handle scientific notation
  const scientificParts = str.split(/[eE]/);
  if (scientificParts.length === 2) {
    const mantissaDecimals = scientificParts[0].split('.')[1]?.length ?? 0;
    const exponent = Number(scientificParts[1]);
    return Number.isFinite(exponent) ? Math.max(0, mantissaDecimals - exponent) : 2;
  }

  // Find decimal point and count digits after
  const dotIndex = str.indexOf('.');
  if (dotIndex === -1) return 0;
  return str.length - dotIndex - 1;
};

export const adjust = (value: number, step: number | string) => {
  const multiplier = 1 / Number(step);
  const adjusted = Math.round(value * multiplier) / multiplier;
  const decimals = afterDecimal(step);
  const factor = pow10(decimals);
  return Math.round(adjusted * factor) / factor;
};

export const add = (a: number, b: number) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = Math.max(aDecimals, bDecimals);
  const factor = pow10(decimals);
  return Math.round((a + b) * factor) / factor;
};

export const subtract = (a: number, b: number) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = Math.max(aDecimals, bDecimals);
  const factor = pow10(decimals);
  return Math.round((a - b) * factor) / factor;
};

// Original multiply function - uses Math.max for decimal precision
// Note: This can lose precision for very small values (e.g., 0.01 * 0.01 = 0)
export const multiply1 = (a: number, b: number) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = Math.max(aDecimals, bDecimals);
  const factor = pow10(decimals);
  return Math.round(a * b * factor) / factor;
};

// Original divide function - uses Math.max for decimal precision
// Note: This can lose precision with integer inputs (e.g., 1 / 3 = 0)
export const divide1 = (a: number, b: number) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = Math.max(aDecimals, bDecimals);
  const factor = pow10(decimals);
  return Math.round((a / b) * factor) / factor;
};

// preserves more precision for multiplication where magnitudes can be small
// for example: 0.0002 * 0.0003 = 0.00000006
export const multiply = (a: number, b: number) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = aDecimals + bDecimals;
  const factor = pow10(decimals);
  return Math.round(a * b * factor) / factor;
};

// preserves more precision for division by using a minimum decimal count
// for example: divide(1, 3) = 0.33333333 instead of 0
export const divide = (a: number, b: number, minDecimals = 8) => {
  const aDecimals = afterDecimal(a);
  const bDecimals = afterDecimal(b);
  const decimals = Math.max(aDecimals, bDecimals, minDecimals);
  const factor = pow10(decimals);
  return Math.round((a / b) * factor) / factor;
};

export const handleRoundingError = (v: number) => {
  if (isNotNumber(v)) {
    return 0;
  }
  return parseFloat(parseFloat(String(v)).toFixed(10));
};

export function round(v: number): number {
  if (isNotNumber(v)) return 0;
  const numV = Number(v);
  const absVal = Math.abs(parseFloat(String(v)));
  let sigDigits = 8;
  if (absVal === 0) {
    sigDigits = 1;
  } else if (absVal >= 1000) {
    sigDigits = 2;
  } else if (absVal >= 10) {
    sigDigits = 3;
  } else if (absVal >= 1) {
    sigDigits = 4;
  } else if (absVal >= 0.0001) {
    sigDigits = 5;
  } else if (absVal >= 0.00001) {
    sigDigits = 6;
  } else if (absVal >= 0.000001) {
    sigDigits = 7;
  } else {
    sigDigits = 8;
  }
  return parseFloat(numV.toFixed(sigDigits));
}
