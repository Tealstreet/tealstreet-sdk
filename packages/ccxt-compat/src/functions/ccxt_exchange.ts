export function parsePrecision(precision?: string) {
  /**
   * @ignore
   * @method
   * @param {string} precision The number of digits to the right of the decimal
   * @returns {string} a string number equal to 1e-precision
   */
  if (precision === undefined) {
    return undefined;
  }
  const precisionNumber = parseInt(precision);
  if (precisionNumber === 0) {
    return '1';
  }
  let parsedPrecision = '0.';
  for (let i = 0; i < precisionNumber - 1; i++) {
    parsedPrecision = parsedPrecision + '0';
  }
  return parsedPrecision + '1';
}

export const number: (numberString: string) => number = Number;

export function parseNumber(value: any, d: number): number {
  if (value === undefined) {
    return d;
  } else {
    try {
      // we should handle scientific notation here
      // so if the exchanges returns 1e-8
      // this function will return 0.00000001
      // check https://github.com/ccxt/ccxt/issues/24135
      const numberNormalized = numberToString(value);
      if (!numberNormalized) {
        return d;
      }
      if (numberNormalized.indexOf('e-') > -1) {
        const res = numberToString(parseFloat(numberNormalized));
        if (!res) {
          return d;
        }
        return number(res);
      }
      return number(numberNormalized);
    } catch {
      return d;
    }
  }
}

export function numberToString(x: any): string | undefined {
  // avoids scientific notation for too large and too small numbers
  if (x === undefined) return undefined;
  if (typeof x !== 'number') return x.toString();
  const s = x.toString();
  if (Math.abs(x) < 1.0) {
    const n_e = s.split('e-');
    const n = n_e[0].replace('.', '');
    const e = parseInt(n_e[1]);
    const neg = s[0] === '-';
    if (e) {
      x = (neg ? '-' : '') + '0.' + new Array(e).join('0') + n.substring(neg as any);
      return x;
    }
  } else {
    const parts = s.split('e');
    if (parts[1]) {
      let e = parseInt(parts[1]);
      const m = parts[0].split('.');
      let part = '';
      if (m[1]) {
        e -= m[1].length;
        part = m[1];
      }
      return m[0] + part + new Array(e + 1).join('0');
    }
  }
  return s;
}

export function parseToInt(number: any) {
  // Solve Common parseInt misuse ex: parseInt ((since / 1000).toString ())
  // using a number as parameter which is not valid in ts
  const stringifiedNumber = numberToString(number) as string;
  const convertedNumber = parseFloat(stringifiedNumber) as any;
  return parseInt(convertedNumber);
}
