import { distributeValues } from './distributeValues';
import { handleRoundingError } from './handleRoundingError';

/**
 * Create a list of amounts that sum to {amount} and are distributed according to {distribution}
 * @param amount
 * @param count
 * @param distribution
 * @param scalePercent
 */
export const distributeAmounts = (
  amount: number,
  count: number,
  distribution: number[],
  scalePercent: number = 1 - 1 / count,
) => {
  if (distribution.length === 0) {
    throw new Error('Distribution cannot be empty');
  }

  // easy out for flat distribution
  if (distribution.every((val, i, arr) => val === arr[0])) {
    const size = handleRoundingError(amount / count);
    return new Array(count).fill(size);
  }

  const referenceDistribution = distributeValues(
    0,
    count + 1,
    100 * Math.max(...distribution) * count + 1,
    distribution,
  );
  const amountDistribution = [];
  let j = 1;
  for (let i = 1; i <= count; i++) {
    let c = 0;
    while (referenceDistribution[j] < i) {
      j += 1;
      c += 1;
    }
    amountDistribution.push(c);
  }

  const amountToScale = amount * scalePercent;
  const amountNotToScale = amount - amountToScale;

  // normalize scale
  const amountMagScale = amountToScale / amountDistribution.reduce((total, curr) => total + curr, 0);
  const amounts = amountDistribution.map((m) => m * amountMagScale + amountNotToScale / count);
  return amounts;
};
