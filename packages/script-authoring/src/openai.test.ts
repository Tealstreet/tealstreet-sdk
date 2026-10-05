import { afterEach, describe, expect, it, vi } from 'vitest';

import { AiProviderError, OpenAiJsonClient } from './openai';

const request = { model: 'm', instructions: 'i', input: 'x', schemaName: 's', schema: {} };

const respond = (body: unknown, ok = true, status = 200) =>
  vi.spyOn(globalThis, 'fetch').mockResolvedValue({ ok, status, json: async () => body } as Response);

const answered = (text: string, usage?: unknown) => ({
  status: 'completed',
  output: [{ type: 'message', content: [{ type: 'output_text', text }] }],
  ...(usage ? { usage } : {}),
});

describe('OpenAiJsonClient usage', () => {
  afterEach(() => vi.restoreAllMocks());

  it('returns the parsed value with the reported token counts', async () => {
    respond(
      answered('{"a":1}', { input_tokens: 120, input_tokens_details: { cached_tokens: 100 }, output_tokens: 30 }),
    );
    const client = new OpenAiJsonClient({ apiKey: 'k' });
    await expect(client.completeWithUsage(request)).resolves.toEqual({
      value: { a: 1 },
      usage: { inputTokens: 120, cachedInputTokens: 100, outputTokens: 30 },
    });
  });

  it('reports no usage rather than zero when OpenAI sent none, and complete() still returns the value', async () => {
    respond(answered('{"a":2}'));
    const client = new OpenAiJsonClient({ apiKey: 'k' });
    await expect(client.completeWithUsage(request)).resolves.toEqual({ value: { a: 2 }, usage: null });
    respond(answered('{"a":3}'));
    await expect(client.complete(request)).resolves.toEqual({ a: 3 });
  });

  it('carries the usage of a billed call that still failed', async () => {
    respond({
      status: 'incomplete',
      incomplete_details: { reason: 'max_output_tokens' },
      usage: { input_tokens: 50, output_tokens: 8000 },
    });
    const error = await new OpenAiJsonClient({ apiKey: 'k' }).completeWithUsage(request).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(AiProviderError);
    expect((error as AiProviderError).usage).toEqual({ inputTokens: 50, cachedInputTokens: 0, outputTokens: 8000 });
  });

  it("sends no reasoning for effort 'none', and keeps today's default otherwise", async () => {
    const fetch = respond(answered('{}'));
    const client = new OpenAiJsonClient({ apiKey: 'k' });
    await client.completeWithUsage({ ...request, effort: 'none' });
    expect(JSON.parse(String(fetch.mock.calls[0][1]?.body))).not.toHaveProperty('reasoning');
    await client.completeWithUsage(request);
    expect(JSON.parse(String(fetch.mock.calls[1][1]?.body)).reasoning).toEqual({ effort: 'low' });
  });
});
