import { describe, expect, it } from 'vitest';
import { ExchangeError, type BingxWriteShapeDetail } from '@tealstreet/safe-cex-wasm/host';
import { classifiedError, deserializeCommandError, exchangeErrorTags, serializeCommandError } from './commandOutcome';

const record = (): BingxWriteShapeDetail => ({
  endpointFamily: 'order', method: 'DELETE', parameterNames: ['orderId', 'signature', 'symbol', 'timestamp'],
  hasOtherParameters: false, idClass: 'numeric', idDigits: 19, targetKind: 'takeProfit',
  symbolFormat: 'hyphenated', replacement: 'unobserved',
});

describe('closed BingX invalid-input shape', () => {
  it('survives the real host, SDK worker clone and classification projection', () => {
    const original = new ExchangeError('fixture refusal', { message: 'fixture refusal', kind: 'invalidInput', venueCode: '109400', bingxWriteShape: record() });
    const encoded = serializeCommandError(original);
    const restored = deserializeCommandError(structuredClone(encoded));
    expect(restored.bingxWriteShape).toEqual(record());
    expect(classifiedError(restored)).toMatchObject({ bingxWriteShape: record() });
    expect(exchangeErrorTags(classifiedError(restored))).toMatchObject({
      error_kind: 'invalidInput', venue_code: '109400', bingx_endpoint: 'order', bingx_method: 'DELETE',
      bingx_param_names_1: 'orderId,signature,symbol,timestamp', bingx_id_class: 'numeric', bingx_id_digits: '19',
      bingx_target_kind: 'takeProfit', bingx_symbol_format: 'hyphenated', bingx_replacement: 'unobserved',
    });
  });
  it('does not copy values, extra properties, or a hostile metadata getter', () => {
    const shape = { ...record(), orderId: 'private-id', symbol: 'PRIVATE-USDT', signature: 'private-signature' };
    const error = Object.assign(new Error('fixture'), {kind: 'invalidInput', bingxWriteShape: shape, request: { key: 'private-key' }});
    const encoded = serializeCommandError(error);
    expect(encoded.bingxWriteShape).toEqual(record());
    expect(JSON.stringify(encoded)).not.toMatch(/private-|PRIVATE-/);
    Object.defineProperty(error, 'bingxWriteShape', {get() {throw new Error('unreadable');}});
    expect(serializeCommandError(error).bingxWriteShape).toBeUndefined();
  });
  it('does not invent a record for success, unrelated failures or malformed values', () => {
    expect(serializeCommandError(new Error('fixture success')).bingxWriteShape).toBeUndefined();
    expect(serializeCommandError({message: 'auth', kind: 'authInvalid', bingxWriteShape: record()}).bingxWriteShape).toBeUndefined();
    expect(serializeCommandError({message:'fixture',kind:'invalidInput',bingxWriteShape:{...record(),parameterNames:['orderId=private-value']}}).bingxWriteShape).toBeUndefined();
  });
});

it('keeps the complete parameter-name set under host tag-length limits', () => {
  const parameterNames: BingxWriteShapeDetail['parameterNames'] = [
    'cancelClientOrderId', 'cancelOrderId', 'cancelReplaceMode', 'cancelRestrictions', 'clientOrderId',
    'closePosition', 'orderId', 'positionId', 'positionSide', 'price', 'priceProtect', 'quantity',
    'reduceOnly', 'side', 'signature', 'stopGuaranteed', 'stopPrice', 'symbol', 'timeInForce', 'timestamp', 'type', 'workingType',
  ];
  const shape: BingxWriteShapeDetail = { ...record(), endpointFamily: 'cancelReplace', method: 'POST', parameterNames };
  const tags = exchangeErrorTags({kind: 'invalidInput', bingxWriteShape: shape});
  const chunks = [tags.bingx_param_names_1, tags.bingx_param_names_2, tags.bingx_param_names_3];
  expect(chunks.every((chunk) => chunk.length < 200)).toBe(true);
  expect(chunks.filter(Boolean).join(',').split(',')).toEqual(parameterNames);
});
