// Explicit live recorder entry point. Tests inject all I/O; do not run captures
// without owner-approved accounts/environments and recording scope.
import { parseArgs } from 'node:util';
import { recordTsPublic } from './record-ts-runtime.mjs';

const { values } = parseArgs({ options: {
  venue: { type: 'string', default: 'binance' },
  seconds: { type: 'string', default: '60' },
  books: { type: 'string', default: 'BTCUSDT,ETHUSDT' },
  out: { type: 'string' }, testnet: { type: 'boolean', default: false },
} });
try {
  const result = await recordTsPublic({ name: values.venue, seconds: Number(values.seconds),
    books: values.books.split(','), out: values.out, testnet: values.testnet });
  console.log(JSON.stringify(result));
  process.exit(0);
} catch (error) {
  console.error(error.message); process.exit(1);
}
