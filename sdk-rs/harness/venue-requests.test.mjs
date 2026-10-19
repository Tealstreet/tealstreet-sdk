import assert from 'node:assert/strict';
import { test } from 'node:test';
import { requestKey } from './requests.mjs';
import { venueRequestKey } from './venue-requests.mjs';
import { venueOf } from './venues.mjs';

test('existing venues retain strict wire types and caller IDs after MEXC integration', () => {
  const url = 'https://fixture.test/order';
  const callerIds = new Set(['caller']);
  for (const name of ['binance', 'bybit', 'okx', 'okxeu', 'okxdemo', 'okxeudemo', 'weex', 'cryptocom', 'bingx']) {
    const venue = venueOf(name);
    const key = (body) => venueRequestKey('POST', url, body, callerIds, venue);
    assert.equal(key({ quantity: 1 }), requestKey('POST', url, { quantity: 1 }, callerIds, venue));
    assert.notEqual(key({ quantity: 1 }), key({ quantity: '1' }));
    assert.notEqual(key({ clientOrderId: 'caller' }), key({ clientOrderId: 'other' }));
    assert.notEqual(key('{"cancelOrderId":8389765519993909000}'), key('{"cancelOrderId":"8389765519993909000"}'));
  }
});

test('only MEXC normalizes native snowflake representations and rolling trigger windows', () => {
  const url = 'https://contract.mexc.com/api/v1/private/order/cancel';
  const venue = venueOf('mexc');
  const key = (body) => venueRequestKey('POST', url, body, new Set(), venue);
  assert.equal(key('{"id":"739113577038255617","vol":2}'), key('{"vol":2,"id":739113577038255617}'));
  assert.notEqual(key('{"id":739113577038255617}'), key('{"id":739113577038255618}'));
  assert.equal(key({ externalOid: 'generated1', vol: 2 }), key({ externalOid: 'generated2', vol: 2 }));
  assert.notEqual(key({ vol: 2 }), key({ vol: '2' }));
  const active = 'https://contract.mexc.com/api/v1/private/planorder/list/orders';
  const read = (query, target = venue) => venueRequestKey('GET', active + query, undefined, new Set(), target);
  assert.equal(read('?start_time=10&end_time=100'), read('?start_time=110&end_time=200'));
  assert.notEqual(read('?start_time=10&end_time=100'), read('?start_time=11&end_time=100'));
  assert.notEqual(read('?start_time=10&end_time=100', venueOf('okx')), read('?start_time=110&end_time=200', venueOf('okx')));
});
