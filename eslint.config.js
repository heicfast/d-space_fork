import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import importx from 'eslint-plugin-import-x';
import globals from 'globals';

const TS_FILES = ['**/*.{ts,tsx}'];

export default tseslint.config(
  {
    ignores: [
      'dist/**',
      'coverage/**',
      'src-tauri/target/**',
      'src-tauri/**',
      'node_modules/**',
      'playwright-report/**',
      'test-results/**',
      'docs/**',
      'worklog/**',
    ],
  },
  eslint.configs.recommended,
  // Type-checked rules only for TS files (needs parserOptions/projectService).
  ...tseslint.configs.recommendedTypeChecked.map((c) => ({ ...c, files: TS_FILES })),
  {
    files: TS_FILES,
    languageOptions: {
      ecmaVersion: 2022,
      globals: { ...globals.browser, ...globals.es2022 },
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      'react-hooks': reactHooks,
      'import-x': importx,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],
      '@typescript-eslint/no-misused-promises': [
        'error',
        { checksVoidReturn: { attributes: false } },
      ],
      '@typescript-eslint/no-floating-promises': ['error', { ignoreIIFE: true }],
      // Architecture boundaries — belt and braces alongside arch-lint + ast-grep.
      'import-x/no-restricted-paths': [
        'error',
        {
          zones: [
            { target: './src/components', from: './src/features' },
            {
              target: './src/features/device-selection',
              from: './src/features/!(device-selection)',
            },
            {
              target: './src/features/lost-partition',
              from: './src/features/!(lost-partition)',
            },
            { target: './src/features/scan-progress', from: './src/features/!(scan-progress)' },
            {
              target: './src/features/results-explorer',
              from: './src/features/!(results-explorer|preview)',
            },
            { target: './src/features/preview', from: './src/features/!(preview)' },
            {
              target: './src/features/recovery-queue',
              from: './src/features/!(recovery-queue)',
            },
            { target: './src/features/health', from: './src/features/!(health)' },
            { target: './src/features/repair', from: './src/features/!(repair)' },
            {
              target: './src/features/settings-about',
              from: './src/features/!(settings-about)',
            },
            { target: './src/features/dev-panel', from: './src/features/!(dev-panel)' },
          ],
        },
      ],
      'no-console': 'error',
    },
  },
  {
    files: ['scripts/**/*.mjs', 'vite.config.ts', 'playwright.config.ts'],
    languageOptions: {
      globals: { ...globals.node },
    },
  },
  {
    // The structured logger is the single sanctioned console surface.
    files: ['src/lib/logger.ts'],
    rules: {
      'no-console': 'off',
    },
  },
  {
    files: [
      'tests/**/*.ts',
      'tests/**/*.tsx',
      'e2e/**/*.ts',
      'scripts/**/*.mjs',
      'vite.config.ts',
      'playwright.config.ts',
    ],
    rules: {
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/only-throw-error': 'off',
      '@typescript-eslint/no-floating-promises': 'off',
      'no-console': 'off',
    },
  },
);
