// ESLint do frontend. Roda dentro do `npm run build`: lint vermelho não gera
// pacote. As regras que importam estão escritas uma a uma, mesmo quando um
// preset já as traz — preset muda de severidade entre versões sem avisar.
import js from "@eslint/js";
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "coverage/**", "public/**"] },

  js.configs.recommended,

  {
    files: ["**/*.{ts,tsx}"],
    extends: [tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
      globals: { ...globals.browser },
    },
  },

  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
      "jsx-a11y": jsxA11y,
    },
    rules: {
      ...jsxA11y.flatConfigs.recommended.rules,
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
      "react-refresh/only-export-components": ["error", { allowConstantExport: true }],
      "no-console": "error",
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/consistent-type-definitions": ["error", "type"],
      "no-restricted-syntax": [
        "error",
        {
          selector: "TSEnumDeclaration",
          message: "Sem `enum`: use união de literais ou objeto `as const`.",
        },
      ],
    },
  },

  // Arquivo de teste não é módulo de componente: exportar utilitário dali não
  // quebra fast refresh nenhum.
  {
    files: ["src/**/*.test.{ts,tsx}", "src/test/**/*.{ts,tsx}"],
    rules: { "react-refresh/only-export-components": "off" },
  },

  {
    files: ["*.js", "*.ts"],
    languageOptions: { globals: { ...globals.node } },
  },

  // Este arquivo não está em nenhum tsconfig: sem tipo para ler.
  {
    files: ["**/*.js"],
    extends: [tseslint.configs.disableTypeChecked],
  },
);
