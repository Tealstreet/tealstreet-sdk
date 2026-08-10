/**
 * Env-driven config for the save-time safety screen.
 *
 * This is the only provider call in the product. We do not generate code for
 * users — their own tooling does — so there is no drafting model, no repair
 * model, and no repair fan-out to bound.
 *
 * Deliberately carries NO request cap. This is a shared library that runs in a
 * multi-instance web deployment, where a per-process counter is neither a real
 * limit nor per-user. The real cap is the persisted per-user daily allowance in
 * the consumer (`script_ai_usage`). A cap here would imply a guarantee it
 * cannot keep, which is worse than no cap at all.
 */
export interface ScreenEnvConfig {
  model: string;
  apiKey: string | null;
  maxOutputTokens: number;
}

const readInt = (raw: string | undefined, fallback: number): number => {
  if (!raw) return fallback;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
};

/**
 * There is no default model id on purpose. Hardcoding one means a wrong or
 * retired id fails at request time with a provider error rather than at
 * startup with a clear message, and it silently pins every deployment to
 * whatever was current when this was written.
 */
export const getScreenConfig = (env: NodeJS.ProcessEnv = process.env): ScreenEnvConfig => {
  const model = env.TEALSTREET_MODULE_SCREEN_MODEL ?? '';
  if (!model) {
    throw new Error('TEALSTREET_MODULE_SCREEN_MODEL is not set — refusing to guess a model id.');
  }
  return {
    model,
    apiKey: env.OPENAI_API_KEY ?? null,
    maxOutputTokens: readInt(env.TEALSTREET_MODULE_SCREEN_MAX_OUTPUT_TOKENS, 8000),
  };
};

export const isScreenConfigured = (config: ScreenEnvConfig): boolean => Boolean(config.apiKey);
