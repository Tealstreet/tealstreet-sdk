const UINT64_MAX = 18_446_744_073_709_551_615n;
const INT64_MAX = 9_223_372_036_854_775_807n;

/** Preserve native account identities, including values above Number.MAX_SAFE_INTEGER. */
export const grvtUint64 = (value: string): string => {
  if (!/^(0|[1-9]\d{0,19})$/.test(value) || BigInt(value) > UINT64_MAX) {
    throw new Error('Invalid GRVT uint64 integer');
  }
  return value;
};

/** Scale the exact human decimal spelling used in the REST leg, without floating multiplication. */
export const grvtScaledUint64 = (value: string, decimals: number): string => {
  const parts = /^(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(value);
  if (
    !parts ||
    !(parts[1].length + (parts[2]?.length ?? 0)) ||
    !Number.isInteger(decimals) ||
    Math.abs(decimals) > 255
  ) {
    throw new Error('Invalid GRVT unsigned decimal');
  }
  const exponent = parts[3] === undefined ? 0 : Number(parts[3]);
  if (!Number.isInteger(exponent) || exponent < -2_147_483_648 || exponent > 2_147_483_647) {
    throw new Error('Invalid GRVT decimal exponent');
  }
  let digits = (parts[1] + (parts[2] ?? '')).replace(/^0+/, '');
  let shift = decimals + exponent - (parts[2]?.length ?? 0);
  while (shift < 0 && digits.endsWith('0')) {
    digits = digits.slice(0, -1);
    shift += 1;
  }
  if (!digits) return '0';
  if (shift < 0) throw new Error('GRVT decimal exceeds native precision');
  if (digits.length + shift > 20) throw new Error('GRVT decimal exceeds uint64');
  const scaled = BigInt(digits) * 10n ** BigInt(shift);
  if (scaled > UINT64_MAX) throw new Error('GRVT decimal exceeds uint64');
  return scaled.toString();
};

export const grvtNanoseconds = (milliseconds: number): string => {
  if (!Number.isSafeInteger(milliseconds) || milliseconds < 0) throw new Error('Invalid GRVT timestamp');
  const nanoseconds = BigInt(milliseconds) * 1_000_000n;
  if (nanoseconds > INT64_MAX) throw new Error('GRVT timestamp exceeds int64');
  return nanoseconds.toString();
};
