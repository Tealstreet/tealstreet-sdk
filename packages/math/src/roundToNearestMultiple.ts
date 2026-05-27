import { handleRoundingError } from './handleRoundingError';

export const roundToNearestMultiple = (n: number, m: number) => handleRoundingError(Math.round(n / m) * m);

export const roundToNearestMultipleDown = (n: number, m: number) => handleRoundingError(Math.floor(n / m) * m);
