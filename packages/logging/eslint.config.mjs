import frontendConfig from '../../eslint.config.frontend.mjs';

export default [
  ...frontendConfig,
  {
    files: ['**/*.ts', '**/*.tsx'],
    rules: {
      // Allow require() for conditional imports (e.g., Sentry in Web Workers)
      '@typescript-eslint/no-require-imports': 'off',
    },
  },
];
