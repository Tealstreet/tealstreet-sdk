import memoize from 'lodash/memoize';

export const getSigDigitsFromPrecision = memoize((n: number) => {
  let strNum = n.toString();

  // Remove scientific notation if present
  if (strNum.includes('e')) {
    let parts = strNum.split('e');
    let exp = parseInt(parts[1], 10);
    strNum =
      exp > 0
        ? parts[0].replace('.', '') + '0'.repeat(exp - (parts[0].length - parts[0].indexOf('.') - 1))
        : '0.' + '0'.repeat(-1 * exp - 1) + parts[0].replace('.', '');
  }

  // Remove any leading zeros and decimal point
  strNum = strNum.replace(/^0+|0+$/g, '').replace('.', '');

  // Find significant digits
  return strNum.length;
});
