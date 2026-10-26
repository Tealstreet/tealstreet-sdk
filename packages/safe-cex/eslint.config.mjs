import frontendConfig from '../../eslint.config.frontend.mjs';
import safeCexPlugin from './eslint-rules/no-discarded-fetch-result.mjs';

export default [
  {
    ignores: [
      // Third-party code that we do not maintain.
      'src/exchanges/apex/starkex-lib/**',
    ],
  },
  ...frontendConfig,
  {
    // The moved TS replay oracle keeps the adapter rule it was written under.
    files: ['src/exchanges/**/*.ts', 'src/__tests__/reference/**/*.ts'],
    plugins: {
      'safe-cex': safeCexPlugin,
    },
    rules: {
      'safe-cex/no-discarded-fetch-result': 'error',
    },
  },
];
