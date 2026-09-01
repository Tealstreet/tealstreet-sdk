import { handleRoundingError } from './handleRoundingError';

/**
 * Floor to a multiple, with an ABSOLUTE tolerance of `m / 100000`.
 *
 * There are two round-down helpers in this package and the difference is a
 * policy, not an accident. This one is for BUCKETING A PRICE — orderbook
 * grouping, footprint and TPO rows, the DOM, the margin slider — where a value
 * a hair under a bucket edge belongs in that bucket. Its tolerance is loose
 * enough to snap a deliberate near-miss: `0.29999999` becomes `0.3`, which its
 * test asserts.
 *
 * `roundToNearestMultipleDown` is for an ORDER SIZE, and uses a RELATIVE 1e-12
 * instead — tight enough to absorb float representation error and nothing
 * else, so `0.29999999` floors to `0.2`. A size really is below the step.
 *
 * Do not swap one for the other. `createScaledOrder` used this one on ladder
 * leg sizes until 2026-08-31, which put the only order-size path on the loose
 * policy.
 *
 * The original comment, kept because it names the bug both now guard against:
 * "need to add in some m / xxxxxx to prevent floating point rounding down to
 * one multiple of m too many".
 */
export const roundDownToMultiple = (n: number, m: number) => handleRoundingError(Math.floor((n + m / 100000) / m) * m);
