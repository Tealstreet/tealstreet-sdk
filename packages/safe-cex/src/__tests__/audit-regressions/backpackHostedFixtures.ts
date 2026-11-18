import { createPrivateKey, sign } from 'node:crypto';

export const STAMP = 1_700_000_000_123;
// Public deterministic seed; OpenSSL is independent of Rust's Ed25519 provider.
export const signature = (payload: string, seed = 7) => {
  const key = createPrivateKey({
    key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, seed)]),
    format: 'der',
    type: 'pkcs8',
  });
  return sign(null, Buffer.from(payload), key).toString('base64');
};

// Every signed method/path in native sign::instruction; ordinary, protective,
// batch and cancel/proof/replacement amend shapes retain their own wire control.
export const endpoints: Array<[string, string, unknown, string]> = [
  ['GET', '/api/v1/account', undefined, 'instruction=accountQuery'],
  ['GET', '/api/v1/position', undefined, 'instruction=positionQuery'],
  ['GET', '/api/v1/capital/collateral', undefined, 'instruction=collateralQuery'],
  ['GET', '/api/v1/orders', undefined, 'instruction=orderQueryAll'],
  ['GET', '/api/v1/orders?symbol=SOL_USDC_PERP', undefined, 'instruction=orderQueryAll&symbol=SOL_USDC_PERP'],
  [
    'GET',
    '/wapi/v1/history/fills?limit=50&offset=0&symbol=SOL_USDC_PERP',
    undefined,
    'instruction=fillHistoryQueryAll&limit=50&offset=0&symbol=SOL_USDC_PERP',
  ],
  [
    'GET',
    '/wapi/v1/history/orders?limit=50&offset=0&symbol=SOL_USDC_PERP',
    undefined,
    'instruction=orderHistoryQueryAll&limit=50&offset=0&symbol=SOL_USDC_PERP',
  ],
  [
    'GET',
    '/wapi/v1/history/funding?limit=50&offset=0&symbol=SOL_USDC_PERP',
    undefined,
    'instruction=fundingHistoryQueryAll&limit=50&offset=0&symbol=SOL_USDC_PERP',
  ],
  [
    'POST',
    '/api/v1/order',
    '{"symbol":"SOL_USDC_PERP","side":"Bid","orderType":"Limit","price":"140.00","quantity":"5.00","clientId":42,"postOnly":true}',
    'instruction=orderExecute&clientId=42&orderType=Limit&postOnly=true&price=140.00&quantity=5.00&side=Bid&symbol=SOL_USDC_PERP',
  ],
  [
    'POST',
    '/api/v1/order',
    '{"symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Market","triggerPrice":"130","triggerQuantity":"5","triggerBy":"LastPrice","reduceOnly":true}',
    'instruction=orderExecute&orderType=Market&reduceOnly=true&side=Ask&symbol=SOL_USDC_PERP&triggerBy=LastPrice&triggerPrice=130&triggerQuantity=5',
  ],
  [
    'POST',
    '/api/v1/order',
    '{"symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Limit","price":"149","triggerPrice":"150","triggerQuantity":"5","triggerBy":"MarkPrice","reduceOnly":true}',
    'instruction=orderExecute&orderType=Limit&price=149&reduceOnly=true&side=Ask&symbol=SOL_USDC_PERP&triggerBy=MarkPrice&triggerPrice=150&triggerQuantity=5',
  ],
  [
    'POST',
    '/api/v1/orders',
    '[{"symbol":"SOL_USDC_PERP","quantity":"5","side":"Bid"},{"symbol":"BTC_USDC_PERP","quantity":"0.01","side":"Ask"}]',
    'instruction=orderExecute&quantity=5&side=Bid&symbol=SOL_USDC_PERP&instruction=orderExecute&quantity=0.01&side=Ask&symbol=BTC_USDC_PERP',
  ],
  [
    'DELETE',
    '/api/v1/order',
    '{"symbol":"SOL_USDC_PERP","orderId":"123"}',
    'instruction=orderCancel&orderId=123&symbol=SOL_USDC_PERP',
  ],
  ['DELETE', '/api/v1/orders', '{"symbol":"SOL_USDC_PERP"}', 'instruction=orderCancelAll&symbol=SOL_USDC_PERP'],
  ['PATCH', '/api/v1/account', '{"leverageLimit":"10"}', 'instruction=accountUpdate&leverageLimit=10'],
];
