import { defineConfig } from "vitest/config";

// Unit tests for web/ (ADR-0056): in Node with jsdom, so StrykerJS can mutate them. What only a browser shows is
// ADR-0054's. Coverage limits follow ADR-0005's tiers: the admin pages' scripts are Tier 1.
export default defineConfig({
  test: {
    include: ["web/src/**/*.test.ts"],
    environment: "jsdom",
    coverage: {
      provider: "v8",
      include: ["web/src/**/*.ts"],
      exclude: ["web/src/**/*.test.ts"],
      reporter: ["text"],
      reportsDirectory: ".tools/coverage-web",
      thresholds: {
        lines: 90,
        branches: 90,
        functions: 90,
        statements: 90,
        // Tier 1. A glob that matches nothing applies no limit and says nothing, so moving this code moves this line.
        "web/src/passkey/**": { lines: 100, branches: 100, functions: 100, statements: 100 },
        "web/src/session/**": { lines: 100, branches: 100, functions: 100, statements: 100 },
        "web/src/scheme/**": { lines: 100, branches: 100, functions: 100, statements: 100 },
        "web/src/menu/**": { lines: 100, branches: 100, functions: 100, statements: 100 },
        // Shared code the admin islands rely on is Tier 1 with them.
        "web/src/shared/**": { lines: 100, branches: 100, functions: 100, statements: 100 },
      },
    },
  },
});
