import frontendConfig from '../../eslint.config.frontend.mjs';

export default [
  {
    ignores: [
      // Third-party code that we do not maintain.
      'src/exchanges/apex/starkex-lib/**',
    ],
  },
  ...frontendConfig,
];
