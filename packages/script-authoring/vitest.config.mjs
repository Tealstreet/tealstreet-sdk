import { defineConfig } from 'vitest/config';

export default defineConfig({
    resolve: { tsconfigPaths: true },
    cacheDir: '../../.cache/vitest/script-authoring',
    test: {
        globals: true,
        environment: 'node',
        include: ['src/**/*.test.ts'],
        // These tests run real TypeScript programs. Whichever one runs first pays
        // for loading the lib .d.ts files, which is ~200ms warm but seconds on a
        // cold CI filesystem — enough to blow the 5s default and fail a run that
        // passes on a retry. The limit is here to catch a hang, not to police
        // compile time.
        testTimeout: 30_000,
        coverage: {
            provider: 'v8',
            reporter: ['text', 'json', 'html'],
            exclude: ['**/node_modules/**', '**/*.test.ts'],
        },
    },
});
