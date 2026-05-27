# CLAUDE.md — @tealstreet/prettierrc

Shared Prettier configuration for the monorepo.

- **Key file**: `prettier.config.js`
- **Settings**: tabWidth=2, printWidth=120, single quotes, trailing commas
- **Import sorting**: Uses `@ianvs/prettier-plugin-sort-imports` with order: types → builtins → react → third-party → @tealstreet → relative
- **JSON5 parser**: Included for tsconfig files
