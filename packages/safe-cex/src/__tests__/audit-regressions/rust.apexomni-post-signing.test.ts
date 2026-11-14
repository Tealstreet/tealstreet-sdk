import { AxiosHeaders } from 'axios';
import { expect, it } from 'vitest';
import { prepareApexOmniRequest } from '../../exchanges/rust/apexomniHostedSigning';
import vectors from '../fixtures/apexomni-post-signing.golden.json';

it.each(vectors)('signs the native semantic POST golden for $label', (vector) => {
  const config = { method: 'post', url: vector.path, data: vector.body,
    headers: new AxiosHeaders({ 'Content-Type': vector.contentType }) };
  expect(prepareApexOmniRequest(config, 1700000000123)?.payload).toBe(vector.preimage);
  expect(config.data).toBe(vector.body);
});

it.each(vectors)('sorts replayed form fields for $label without changing transport bytes', (vector) => {
  const data = new URLSearchParams([...vector.fields].sort(([a], [b]) => a < b ? 1 : -1) as [string, string][]).toString();
  const config = { method: 'post', url: vector.path, data,
    headers: new AxiosHeaders({ 'Content-Type': 'application/x-www-form-urlencoded' }) };
  expect(prepareApexOmniRequest(config, 1700000000123)?.payload).toBe(vector.preimage);
  expect(config.data).toBe(data);
});

it.each(vectors)('signs semantic objects for $label with the same native field ordering', (vector) => {
  const data = Object.fromEntries(vector.fields);
  const config = { method: 'post', url: vector.path, data, headers: new AxiosHeaders() };
  expect(prepareApexOmniRequest(config, 1700000000123)?.payload).toBe(vector.preimage);
  expect(config.data).toBe(data);
});

it('signs FormData and URLSearchParams fields without transport-specific internals', () => {
  const vector = vectors[0];
  const form = new FormData();
  for (const [key, value] of vector.fields) form.append(key, value);
  for (const data of [form, new URLSearchParams(vector.fields as [string, string][])]) {
    const config = { method: 'post', url: vector.path, data, headers: new AxiosHeaders() };
    expect(prepareApexOmniRequest(config, 1700000000123)?.payload).toBe(vector.preimage);
  }
});

it('does not sign multipart boundaries and accepts quoted boundary parameters', () => {
  const vector = vectors[0];
  const boundary = 'different-form-boundary';
  const data = vector.body.replaceAll('safe-cex-apexomni', boundary);
  expect(prepareApexOmniRequest({ method: 'post', url: vector.path, data,
    headers: new AxiosHeaders({ 'Content-Type': `multipart/form-data; boundary="${boundary}"` }) }, 1700000000123)?.payload)
    .toBe(vector.preimage);
});

it('refuses malformed multipart, invalid form encoding and non-text fields', () => {
  const vector = vectors[0];
  const file = new FormData(); file.append('file', new Blob(['not a scalar field']));
  for (const [data, contentType] of [
    [vector.body, 'multipart/form-data'],
    [vector.body.slice(0, -5), vector.contentType],
    ['id=%ZZ', 'application/x-www-form-urlencoded'],
    [file, 'multipart/form-data'],
    [{ order: { size: 1 } }, 'application/json'],
  ]) expect(() => prepareApexOmniRequest({ method: 'post', url: vector.path, data,
    headers: new AxiosHeaders({ 'Content-Type': contentType as string }) }, 1700000000123)).toThrow();
});
