// Independent oracle: Node's built-in crypto, never safe-cex or Rust code.
const { createHmac, createHash } = require('node:crypto');
const { writeFileSync } = require('node:fs');
const { join } = require('node:path');
const secret = 'test-secret-value';
const hmac = (algorithm, key, payload, encoding) => createHmac(algorithm, key).update(payload).digest(encoding);
const hash = payload => createHash('sha256').update(payload).digest();
const ws = [
  ['hex', 'GET/realtime1700000000', hmac('sha256', secret, 'GET/realtime1700000000', 'hex')],
  ['base64', '1700000000GET/users/self/verify', hmac('sha256', secret, '1700000000GET/users/self/verify', 'base64')],
  ['hex-base64', '/users/self/verifyGET1700000000abc-nonce', Buffer.from(hmac('sha256', secret, '/users/self/verifyGET1700000000abc-nonce', 'hex')).toString('base64')],
  ['apexomni-base64', '1700000000GET/ws/accounts', hmac('sha256', Buffer.from(secret).toString('base64'), '1700000000GET/ws/accounts', 'base64')],
  ['bitunix-double-sha256', 'nonce-abc1700000000api-key-xyz', hash(hash('nonce-abc1700000000api-key-xyz').toString('hex') + secret).toString('hex')],
].map(([format, payload, signature]) => ({ format, payload, secret, signature })).concat([
  { format: 'kraken-challenge', payload: 'challenge-token', secret, error: 'kraken-challenge requires the KrakenExchange signWsAuth override' },
  { format: 'coinbase-jwt', payload: 'coinbase-websocket-jwt.user', secret, error: 'coinbase-jwt requires the CoinbaseExchange signWsAuth override' },
]);
const kraken = ['dGVzdC1zZWNyZXQtdmFsdWU=', 'dGVzdC1zZWNyZXQtdmFsdWU', ' dGVz\ndC1z!ZWNyZXQtdmFsdWU=ignored', '-_8=', '+/_-', '====', 'a', 'a=Zm9v', '!!!!', 'dGVzdA===more', 'Zm😀9v', 'ZmŁ9v']
  .map(secret => ({ secret, payload: 'challenge-token雪\0', signature: hmac('sha512', Buffer.from(secret, 'base64'), hash('challenge-token雪\0'), 'base64') }));
const primitives = [];
for (const algorithm of ['sha256', 'sha512']) {
  for (const [key, payload] of [['', ''], ['κλειδί🔑', 'payload\0雪'], ['x'.repeat(200), 'long-key']]) {
    for (const encoding of ['hex', 'base64']) primitives.push({ algorithm, key, payload, encoding, signature: hmac(algorithm, key, payload, encoding) });
  }
}
// Oracle protocol coercion with native JS, independent of Rust's Value helpers.
const formats = [123, {}, [], true, false, 0, -0, null, '', ['hex'],
  [null, ['hex'], {}], 1e21, 1e-7, 0.000001, 1e20, 9007199254740993];
const wire = [];
const wireInputs = formats.map(outputFormat => ['pending-wrong-type', outputFormat])
  .concat([['pending-wrong-type', undefined]])
  .concat([42, null, true, {}, [], undefined].map(id => [id, 123]));
for (const [id, outputFormat] of wireInputs) {
  const request = { id, wsAuth: { toSign: 'challenge-token', outputFormat } };
  const response = { id, error: !outputFormat
    ? 'Missing wsAuth payload for WS auth signing'
    : `Unsupported WS auth outputFormat: ${String(outputFormat)}` };
  wire.push({ request, response, kraken: { id, wsAuthSignature:
    hmac('sha512', Buffer.from('c2VjcmV0', 'base64'), hash('challenge-token'), 'base64') } });
}
// Keep raw JSON text: JSON.stringify cannot encode an out-of-range numeric token.
const ignoredTokens = ['1e400', '-1e400', '1' + '0'.repeat(400),
  '['.repeat(200) + '1e400' + ']'.repeat(200),
  '{"nested":'.repeat(200) + '1e400' + '}'.repeat(200)];
const text = { signable: [], retained: [], invalidPayload: [] };
const textResponse = request => ({ id: request.id, wsAuthSignature:
  hmac('sha256', secret, request.wsAuth.toSign, 'hex') });
const textKraken = request => ({ id: request.id, wsAuthSignature:
  hmac('sha512', Buffer.from('c2VjcmV0', 'base64'), hash(request.wsAuth.toSign), 'base64') });
for (const token of ignoredTokens) {
  for (const wire of [
    `{"extraData":{"expiresAt":${token}},"id":"text-1","wsAuth":{"toSign":"GET/realtime1700000000","outputFormat":"hex"}}`,
    `{"id":"text-1","wsAuth":{"extraData":{"expiresAt":${token}},"toSign":"GET/realtime1700000000","outputFormat":"hex"}}`,
    `{"wsAuth":{"toSign":"GET/realtime1700000000","outputFormat":"hex","extraData":${token}},"id":"text-1","unused":${token}}`,
  ]) {
    const request = JSON.parse(wire);
    text.signable.push({ wire, response: textResponse(request), kraken: textKraken(request) });
  }
}
for (const token of ['1e400', '['.repeat(200) + '0' + ']'.repeat(200)]) {
  for (const field of ['id', 'outputFormat']) {
    const wire = field === 'id'
      ? `{"id":${token},"wsAuth":{"toSign":"GET/realtime1700000000","outputFormat":"hex"}}`
      : `{"id":"retained-1","wsAuth":{"toSign":"GET/realtime1700000000","outputFormat":${token}}}`;
    const request = JSON.parse(wire);
    text.retained.push({ wire, limit: token === '1e400' ? 'overflow' : 'depth',
      tsResponseJson: JSON.stringify(field === 'id' ? textResponse(request) : { id: request.id,
        error: `Unsupported WS auth outputFormat: ${String(request.wsAuth.outputFormat)}` }),
      tsKrakenJson: JSON.stringify(textKraken(request)) });
  }
}
// Invalid payloads are rejected by the Rust projection after parsing, not by its parser.
for (const token of ['1e400', '['.repeat(200) + '0' + ']'.repeat(200)]) {
  for (const field of ['toSign', 'wsAuth']) {
    const wire = field === 'toSign'
      ? `{"id":"invalid-payload","wsAuth":{"toSign":${token},"outputFormat":"hex"}}`
      : `{"id":"invalid-payload","wsAuth":${token}}`;
    text.invalidPayload.push({ wire, limit: token === '1e400' ? 'overflow' : 'depth',
      response: { id: 'invalid-payload', error: 'Missing wsAuth payload for WS auth signing' },
      kraken: { id: 'invalid-payload', error: 'Missing challenge to sign' } });
  }
}
const nonfinite = ['[1e400]', '[-1e400]', '[[1e400]]', '[1e309]', '1e400', '-1e400']
  .map(outputFormat => {
    const wire = `{"id":"overflow-format","wsAuth":{"toSign":"challenge","outputFormat":${outputFormat}}}`;
    const request = JSON.parse(wire);
    return { wire, response: { id: request.id,
      error: `Unsupported WS auth outputFormat: ${String(request.wsAuth.outputFormat)}` }, kraken: textKraken(request) };
  });
writeFileSync(join(__dirname, 'node-crypto.json'), JSON.stringify({ ws, kraken, primitives, wire, text, nonfinite }, null, 2) + '\n');
