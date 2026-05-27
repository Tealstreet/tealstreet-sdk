import { handleRoundingError } from './handleRoundingError';

export const roundUpToMultiple = (n: number, m: number) => handleRoundingError(Math.ceil(n / m) * m);
