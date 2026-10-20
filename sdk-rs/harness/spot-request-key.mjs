import { requestKey } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';

// Wrap the verified payload to avoid unrelated venues' root clock/ID defaults.
// The protocol projection removes only entropy authenticated for this route.
export const spotRequestKey = (method, url, body, callerIds = new Set(), project = (value) => value, generatedBodyId) => {
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  let parsed = body === undefined || body === null || body === '' ? {} : body;
  if (body instanceof URLSearchParams || (typeof body === 'string' && !body.trimStart().startsWith('{') && body.includes('=')))
    parsed = Object.fromEntries(new URLSearchParams(body));
  else if (typeof parsed === 'string') parsed = parseRequestJson(parsed);
  const context = { method, url, callerIds };
  return requestKey(method, url, [project(parsed, context)], callerIds, {
    generatedId: () => false, generatedBodyId,
    preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  });
};
