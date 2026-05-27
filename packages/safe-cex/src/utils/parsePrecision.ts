export const parsePrecision = (precision?: string) => {
  if (precision === undefined) {
    return undefined;
  }
  const precisionNumber = parseInt(precision, 10);
  if (isNaN(precisionNumber) || precisionNumber < 0) {
    return undefined;
  }
  if (precisionNumber === 0) {
    return '1';
  }
  return '0.' + '0'.repeat(precisionNumber - 1) + '1';
};
