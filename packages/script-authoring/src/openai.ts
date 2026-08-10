/**
 * Minimal OpenAI JSON transport.
 *
 * This is the drafting client's transport with the drafting removed. We do not
 * generate code for users — their own tooling does that — so the only provider
 * call left in the product is the save-time safety screen, which needs the same
 * request shape against a different response schema.
 */

export class AiConfigurationError extends Error {
  constructor(message = 'AI provider is not configured') {
    super(message);
    this.name = 'AiConfigurationError';
  }
}

export class AiProviderError extends Error {
  status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = 'AiProviderError';
    this.status = status;
  }
}

export interface OpenAiJsonRequest {
  model: string;
  instructions: string;
  input: string;
  /** Schema name surfaced to the provider; must match `^[a-zA-Z0-9_-]+$`. */
  schemaName: string;
  /**
   * OpenAI strict mode requires every property listed in `required` and
   * `additionalProperties: false` at every level, including nested objects.
   */
  schema: Record<string, unknown>;
  effort?: 'low' | 'medium' | 'high';
  /** Versioned by the caller: a prompt change must not reuse a cache entry
   *  built from the previous instructions. */
  cacheKey?: string;
  metadata?: Record<string, string>;
}

export interface OpenAiClientConfig {
  apiKey: string | null;
  /**
   * Reasoning tokens count against `max_output_tokens`, so a small budget
   * truncates silently and surfaces as "did not include output text" — a
   * confusing failure a long way from its cause.
   */
  maxOutputTokens?: number;
}

interface OpenAiResponse {
  status?: string;
  incomplete_details?: { reason?: string } | null;
  output?: Array<{ type?: string; content?: Array<Record<string, unknown>> }>;
  error?: { message?: string } | null;
}

const isOutputText = (value: unknown): value is { type: 'output_text'; text: string } =>
  typeof value === 'object' &&
  value !== null &&
  (value as { type?: unknown }).type === 'output_text' &&
  typeof (value as { text?: unknown }).text === 'string';

const isRefusal = (value: unknown): value is { type: 'refusal'; refusal: string } =>
  typeof value === 'object' &&
  value !== null &&
  (value as { type?: unknown }).type === 'refusal' &&
  typeof (value as { refusal?: unknown }).refusal === 'string';

const getOutputText = (response: OpenAiResponse): string => {
  for (const output of response.output ?? []) {
    for (const item of output.content ?? []) {
      if (isRefusal(item)) throw new AiProviderError(item.refusal);
      if (isOutputText(item)) return item.text;
    }
  }
  throw new AiProviderError('OpenAI response did not include output text');
};

/** Returns the parsed JSON body. Shape validation belongs to the caller. */
export class OpenAiJsonClient {
  constructor(private readonly config: OpenAiClientConfig) {}

  async complete(request: OpenAiJsonRequest): Promise<unknown> {
    if (!this.config.apiKey) throw new AiConfigurationError();

    const response = await fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${this.config.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: request.model,
        instructions: request.instructions,
        input: [{ role: 'user', content: [{ type: 'input_text', text: request.input }] }],
        max_output_tokens: this.config.maxOutputTokens ?? 8000,
        reasoning: { effort: request.effort ?? 'low' },
        store: false,
        ...(request.cacheKey ? { prompt_cache_key: request.cacheKey } : {}),
        text: {
          format: { type: 'json_schema', name: request.schemaName, strict: true, schema: request.schema },
        },
        ...(request.metadata ? { metadata: request.metadata } : {}),
      }),
    });

    const body = (await response.json().catch(() => null)) as OpenAiResponse | null;
    if (!response.ok) {
      throw new AiProviderError(
        body?.error?.message ?? `OpenAI request failed with status ${response.status}`,
        response.status,
      );
    }
    if (!body) throw new AiProviderError('OpenAI returned an empty response');
    if (body.status === 'incomplete') {
      throw new AiProviderError(
        `OpenAI response was truncated (${body.incomplete_details?.reason ?? 'unknown reason'}). Raise maxOutputTokens.`,
      );
    }

    try {
      return JSON.parse(getOutputText(body)) as unknown;
    } catch (error) {
      if (error instanceof AiProviderError) throw error;
      throw new AiProviderError(
        error instanceof Error ? `OpenAI returned malformed JSON: ${error.message}` : 'OpenAI returned malformed JSON',
      );
    }
  }
}
