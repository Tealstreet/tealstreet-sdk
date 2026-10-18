// Regenerate venue-standard-methods.json with Node and the installed source Dayjs.
const path = require('node:path');
const source = process.env.TYPESCRIPT_PACKAGE_DIR || path.resolve(__dirname, '../../../../packages/safe-cex');
const load = name => require(require.resolve(name, { paths: [source] }));
const dayjs = load('dayjs');
dayjs.extend(load('dayjs/plugin/utc'));
const BigNumber = load('bignumber.js');
const families = {
  RegExp: RegExp.prototype, Date: Date.prototype, Boolean: Boolean.prototype,
  Error: Error.prototype, MathStatic: Math, PromiseStatic: Promise,
  Array: Array.prototype, String: String.prototype, Number: Number.prototype,
  Map: Map.prototype, Set: Set.prototype, Promise: Promise.prototype,
  Function: Function.prototype, Object: Object.prototype,
  ObjectStatic: Object, NumberStatic: Number, ArrayStatic: Array,
  Dayjs: Object.getPrototypeOf(dayjs(0)),
  JSONStatic: JSON, DateStatic: Date, BigNumber: BigNumber.prototype,
};
const catalog = Object.fromEntries(Object.entries(families).map(([family, value]) => [family,
  Object.getOwnPropertyNames(value).filter(name => name !== 'constructor' &&
    typeof Object.getOwnPropertyDescriptor(value, name)?.value === 'function'),
]));
process.stdout.write(`${JSON.stringify(catalog, null, 2)}\n`);
