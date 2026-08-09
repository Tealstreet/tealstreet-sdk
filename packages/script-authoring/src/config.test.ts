import { describe, expect, it } from 'vitest';

import { getScreenConfig, isScreenConfigured } from './config';

describe('getScreenConfig', () => {
  // Failing at startup with a clear message beats failing at request time with
  // a provider error about an unknown model.
  it('refuses to guess a model id', () => {
    expect(() => getScreenConfig({} as NodeJS.ProcessEnv)).toThrow(/TEALSTREET_MODULE_SCREEN_MODEL/);
  });

  it('reads the model id from the environment', () => {
    expect(getScreenConfig({ TEALSTREET_MODULE_SCREEN_MODEL: 'm1' } as NodeJS.ProcessEnv).model).toBe('m1');
  });

  // Reasoning tokens count against this; too small truncates silently.
  it('defaults maxOutputTokens high enough for reasoning plus a verdict', () => {
    expect(getScreenConfig({ TEALSTREET_MODULE_SCREEN_MODEL: 'm' } as NodeJS.ProcessEnv).maxOutputTokens).toBe(8000);
  });

  it('ignores a junk token budget rather than producing NaN', () => {
    const config = getScreenConfig({
      TEALSTREET_MODULE_SCREEN_MODEL: 'm',
      TEALSTREET_MODULE_SCREEN_MAX_OUTPUT_TOKENS: 'not-a-number',
    } as NodeJS.ProcessEnv);
    expect(config.maxOutputTokens).toBe(8000);
  });

  it('reports whether a key is present without exposing it', () => {
    expect(isScreenConfigured(getScreenConfig({ TEALSTREET_MODULE_SCREEN_MODEL: 'm' } as NodeJS.ProcessEnv))).toBe(
      false,
    );
    expect(
      isScreenConfigured(
        getScreenConfig({ TEALSTREET_MODULE_SCREEN_MODEL: 'm', OPENAI_API_KEY: 'sk-x' } as NodeJS.ProcessEnv),
      ),
    ).toBe(true);
  });
});
