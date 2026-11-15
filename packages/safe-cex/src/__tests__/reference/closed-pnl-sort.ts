import type { ClosedPnlEntry } from '../../types';

// Retained reference callers share the new nullable contract without coercing dates.
export const compareClosedPnlTimes = (a: ClosedPnlEntry, b: ClosedPnlEntry): number => {
  if (a.closedTime === null) return b.closedTime === null ? 0 : 1;
  if (b.closedTime === null) return -1;
  return b.closedTime - a.closedTime;
};
