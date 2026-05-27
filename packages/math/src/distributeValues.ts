import { handleRoundingError } from './handleRoundingError';

/**
 * create list of values between {start} and {end} with {count} entries and according to a {distribution}
 * described by an estimated polynomial fitting the distribution.
 * This estimate is based on rectangle integral estimate of the polynomial directly from the points in the distribution.
 * @param start
 * @param end
 * @param count
 * @param distribution
 */
export const distributeValues = (start: number, end: number, count: number, distribution: number[]) => {
  if (distribution.length === 0) {
    throw new Error('Distribution cannot be empty');
  }

  const valDiff = end - start;
  // valDiff = valDiff + valDiff/(numEntries-1)
  const priceSegmentOffset = valDiff / distribution.length;

  const rectWidth = (count - 1) / distribution.reduce((total, curr) => total + curr, 0);
  const distributedVals: number[] = [];
  let remainder = 0;

  distribution.forEach((mag, index) => {
    const numEntries = mag * rectWidth;
    const prevIncr = priceSegmentOffset / numEntries;

    const startPrice = start + priceSegmentOffset * index;
    for (let i = remainder % 1; i < numEntries; i++) {
      distributedVals.push(startPrice + i * prevIncr);
    }
    remainder = Math.ceil(numEntries) + remainder - numEntries;
  });

  if (distributedVals.length === handleRoundingError(count)) {
    distributedVals.pop();
  } else if (distributedVals.length !== count - 1) {
    console.log(
      `ERROR: Wrong number of entries... found ${distributedVals.length + 1} expected ${handleRoundingError(count)}`,
    );
  }
  distributedVals.push(end);
  return distributedVals;
};
