import assert from 'node:assert/strict';
import test from 'node:test';
import { requestFailureFields } from './request-failure.mjs';

test('path templates retain only fixed API words, including for escaped or secret-bearing segments', () => {
  for (const secret of ['SECRET', 'a61b5421-uuid', '0x123456789', '%53ECRET', 'key=SECRET', '%7B%22token%22%3A%22SECRET%22%7D']) {
    const facts = requestFailureFields({ method: 'GET', url: `https://USER:SECRET@api.bybit.com/v5/order/${secret}?signature=SECRET&timestamp=123#SECRET` });
    assert.deepEqual(facts, { method: 'GET', path: '/v5/order/:segment' });
    assert.doesNotMatch(JSON.stringify(facts), /SECRET|bybit\.com|USER|timestamp|signature/);
  }
});
test('malformed URLs, methods and arbitrarily deep paths stay bounded without caller text', () => {
  assert.deepEqual(requestFailureFields({ method: 'SECRET', url: 'not an URL SECRET' }), { method: 'OTHER', path: '/:unknown' });
  const facts = requestFailureFields({ method: 'POST', url: 'https://venue.test/' + 'instruments-info/'.repeat(1000) + 'SECRET' });
  assert.ok(facts.path.length <= 160);
  assert.equal(facts.path.split('/').filter(Boolean).length, 9);
  assert.ok(facts.path.endsWith('/:more'));
  assert.doesNotMatch(facts.path, /SECRET/);
});
