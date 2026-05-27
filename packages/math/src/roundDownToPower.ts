export const roundDownToPower = (a: number, b: number) => {
  if (a <= 0 || b <= 1) throw new Error('a must be > 0 and b must be > 1');

  const exponent = Math.round(Math.log(a) / Math.log(b));
  return Math.pow(b, exponent);
};
