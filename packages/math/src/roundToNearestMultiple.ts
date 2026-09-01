import { handleRoundingError } from './handleRoundingError';

export const roundToNearestMultiple = (n: number, m: number) => handleRoundingError(Math.round(n / m) * m);

/**
 * Floor to a multiple — but treat a quotient that is an integer within float
 * noise AS that integer.
 *
 * A plain `Math.floor(n / m)` is wrong for values a user actually types:
 * `0.3 / 0.1` is `2.9999999999999996`, so flooring gave `2` and
 * `roundToNearestMultipleDown(0.3, 0.1)` returned **0.2** — a third of the
 * order silently dropped, on an input that is an exact multiple of the step.
 * `0.7`, `2.9`, `8.2` and `0.29 / 0.01` all did the same.
 *
 * That is not theoretical: this is the round-DOWN branch of `roundOrderSize`,
 * which Hyperliquid SPOT placement uses precisely so a buy cannot exceed the
 * balance held. Under-sizing by a full lot step is the opposite of a safe
 * failure there, and it reached the venue.
 *
 * The tolerance is relative so it holds at large quotients too. A genuine
 * round-down is untouched: `0.35 / 0.1` is `3.4999…`, nowhere near an integer,
 * and still floors to 3.
 *
 * **Not the same policy as `roundDownToMultiple`**, which uses an ABSOLUTE
 * `m / 100000` and therefore also snaps a deliberate near-miss (`0.29999999`
 * to `0.3`). That is right for bucketing a price and wrong for sizing an
 * order: this one floors `0.29999999` to `0.2`, because a size really is
 * below the step. See that function for which job takes which.
 */
const QUOTIENT_EPSILON = 1e-12;

export const roundToNearestMultipleDown = (n: number, m: number) => {
  const quotient = n / m;
  const nearest = Math.round(quotient);
  const isExactMultiple = Math.abs(quotient - nearest) <= Math.max(1, Math.abs(quotient)) * QUOTIENT_EPSILON;
  return handleRoundingError((isExactMultiple ? nearest : Math.floor(quotient)) * m);
};
