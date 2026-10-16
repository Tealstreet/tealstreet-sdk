// Native Spot's typed order/read payloads emit flat form fields. Observe the
// source's actual qs dependency across the stringify -> parse -> re-sign path.
const qs = require('../../../node_modules/qs');
const variants = [
  { pair: 'XBTUSD', type: 'buy', ordertype: 'limit', volume: '0.501', price: '30000', cl_ord_id: 'Ts-123' },
  { pair: 'XBT/USD', type: 'sell', ordertype: 'trailing-stop', volume: '17.25', price: '+12.5' },
  { pair: 'BTC/£?&=+', cl_ord_id: 'a b+c%2B=é😀', volume: '0.000001' },
  { oflags: 'post,fcib', volume: '1', validate: false, userref: 0 },
  { txid: ['a', 'b'], nonce: ['stale-a', 'stale-b'] },
  {},
];
process.stdout.write(JSON.stringify(variants.map((fields) => {
  const input = qs.stringify({ ...fields, nonce: fields.nonce || 'previous' }, { arrayFormat: 'repeat' });
  const { nonce: previous, ...rest } = qs.parse(input);
  return { input, expected: qs.stringify({ ...rest, nonce: 'next' }, { arrayFormat: 'repeat' }) };
})));
