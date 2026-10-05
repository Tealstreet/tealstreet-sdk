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

/** What a call cost, as OpenAI reports it. `cachedInputTokens` is a SUBSET of
 *  `inputTokens`; reasoning tokens are already inside `outputTokens`. */
export interface OpenAiUsage {
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
}

export class AiProviderError extends Error {
  status?: number;
  /** Set when the provider billed for a call that still failed (a truncated
   *  answer, malformed JSON), so a caller charging by usage can settle it. */
  usage?: OpenAiUsage;

  constructor(message: string, status?: number, usage?: OpenAiUsage) {
    super(message);
    this.name = 'AiProviderError';
    this.status = status;
    if (usage) this.usage = usage;
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
  /** `'none'` sends no `reasoning` at all — non-reasoning models (gpt-4.1, gpt-4o) reject it. */
  effort?: 'none' | 'low' | 'medium' | 'high';
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
  /** Abort a call that has not answered in time; unset waits forever. A timeout
   *  carries no status, so a credit-charging caller keeps its reservation. */
  timeoutMs?: number;
}

interface OpenAiResponse {
  status?: string;
  usage?: {
    input_tokens?: number;
    input_tokens_details?: { cached_tokens?: number } | null;
    output_tokens?: number;
  } | null;
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

const count = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0);

const readUsage = (response: OpenAiResponse | null): OpenAiUsage | undefined => {
  const usage = response?.usage;
  if (!usage) return undefined;
  return {
    inputTokens: count(usage.input_tokens),
    cachedInputTokens: count(usage.input_tokens_details?.cached_tokens),
    outputTokens: count(usage.output_tokens),
  };
};

const getOutputText = (response: OpenAiResponse): string => {
  for (const output of response.output ?? []) {
    for (const item of output.content ?? []) {
      if (isRefusal(item)) throw new AiProviderError(item.refusal, undefined, readUsage(response));
      if (isOutputText(item)) return item.text;
    }
  }
  throw new AiProviderError('OpenAI response did not include output text', undefined, readUsage(response));
};

/** Returns the parsed JSON body. Shape validation belongs to the caller. */
export class OpenAiJsonClient {
  constructor(private readonly config: OpenAiClientConfig) {}

  async complete(request: OpenAiJsonRequest): Promise<unknown> {
    return (await this.completeWithUsage(request)).value;
  }

  /** `usage` is null only when OpenAI did not report one. */
  async completeWithUsage(request: OpenAiJsonRequest): Promise<{ value: unknown; usage: OpenAiUsage | null }> {
    if (!this.config.apiKey) throw new AiConfigurationError();

    const response = await fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      ...(this.config.timeoutMs ? { signal: AbortSignal.timeout(this.config.timeoutMs) } : {}),
      headers: {
        Authorization: `Bearer ${this.config.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        model: request.model,
        instructions: request.instructions,
        input: [{ role: 'user', content: [{ type: 'input_text', text: request.input }] }],
        max_output_tokens: this.config.maxOutputTokens ?? 8000,
        ...(request.effort === 'none' ? {} : { reasoning: { effort: request.effort ?? 'low' } }),
        store: false,
        ...(request.cacheKey ? { prompt_cache_key: request.cacheKey } : {}),
        text: {
          format: { type: 'json_schema', name: request.schemaName, strict: true, schema: request.schema },
        },
        ...(request.metadata ? { metadata: request.metadata } : {}),
      }),
    });

    const body = (await response.json().catch(() => null)) as OpenAiResponse | null;
    const usage = readUsage(body);
    if (!response.ok) {
      throw new AiProviderError(
        body?.error?.message ?? `OpenAI request failed with status ${response.status}`,
        response.status,
        usage,
      );
    }
    if (!body) throw new AiProviderError('OpenAI returned an empty response');
    if (body.status === 'incomplete') {
      throw new AiProviderError(
        `OpenAI response was truncated (${body.incomplete_details?.reason ?? 'unknown reason'}). Raise maxOutputTokens.`,
        undefined,
        usage,
      );
    }

    try {
      return { value: JSON.parse(getOutputText(body)) as unknown, usage: usage ?? null };
    } catch (error) {
      if (error instanceof AiProviderError) throw error;
      throw new AiProviderError(
        error instanceof Error ? `OpenAI returned malformed JSON: ${error.message}` : 'OpenAI returned malformed JSON',
        undefined,
        usage,
      );
    }
  }
}
