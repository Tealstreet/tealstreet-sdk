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
    files: ['src/exchanges/**/*.ts'],
    plugins: {
      'safe-cex': safeCexPlugin,
    },
    rules: {
      'safe-cex/no-discarded-fetch-result': 'error',
    },
  },
];
