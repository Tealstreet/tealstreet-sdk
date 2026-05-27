import { handleRoundingError } from './handleRoundingError';

// need to add in some m / xxxxxx to prevent floating point rounding down to one multiple of m too many
export const roundDownToMultiple = (n: number, m: number) => handleRoundingError(Math.floor((n + m / 100000) / m) * m);
