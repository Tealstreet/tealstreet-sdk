const assert = require('node:assert/strict');
const path = require('node:path');
const { root, load } = require('../../safe-cex-types/scripts/accounts-source.cjs');
const calendar = load(path.join(root, 'packages/safe-cex/src/utils/calendarCandles.ts'));
const yearOne = -62135596800;
const yearHundred = -59011459200;
const cases = [
  ['start-year-one', () => calendar.chartCandleStartTime('1M', yearOne), -2177452800],
  ['next-year-one', () => calendar.nextChartCandleStartTime('1M', yearOne), -2174774400],
  ['close-year-one', () => calendar.chartCandleCloseTime('1M', yearOne), -2174774400],
  ['count-year-one-to-hundred', () => calendar.getNumChartCandlesInRange('1M', yearOne, yearHundred), -21611],
  ['start-NaN', () => calendar.chartCandleStartTime('1M', NaN), NaN],
  ['next-NaN', () => calendar.nextChartCandleStartTime('1M', NaN), NaN],
  ['count-NaN', () => calendar.getNumChartCandlesInRange('1M', NaN, 0), NaN],
  ['start-epoch', () => calendar.chartCandleStartTime('1M', 0), 0],
  ['next-epoch', () => calendar.nextChartCandleStartTime('1M', 0), 2678400],
  ['close-epoch', () => calendar.chartCandleCloseTime('1M', 0), 2678400],
  ['count-epoch', () => calendar.getNumChartCandlesInRange('1M', 0, 2678400), 2],
];
const rows = cases.map(([name, run, expected]) => {
  const actual = run();
  assert.equal(typeof actual, 'number', 'CORE-CALENDAR source numeric ' + name);
  assert(Object.is(actual, expected), 'CORE-CALENDAR source value ' + name);
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(Number.isNaN(actual) ? NaN : actual);
  return { name, bits: bytes.toString('hex') };
});
assert.equal(rows.length, 11, 'CORE-CALENDAR independent source floor');
process.stdout.write(JSON.stringify(rows));
