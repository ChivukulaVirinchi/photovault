import { defineConfig } from "vitest/config";

// Vitest config. Kept separate from `vite.config.ts` so the dev/build
// pipeline doesn't pull in any test-only plugins. Vitest reads this
// automatically when run from the `src-ui` directory.
//
// Conventions for tests under `src/`:
//   - Plain utility tests use `node` environment (default) — fast,
//     no DOM, no jsdom overhead.
//   - Component tests should set `// @vitest-environment jsdom` at
//     the top of the file (we don't enable jsdom globally because
//     it's slow to spin up and most of our tests don't need it).
//   - Test files end in `.test.ts` and live next to the code they
//     exercise. svelte-check ignores them (they're outside the
//     production type-check path).
export default defineConfig({
  test: {
    // Default environment for utility tests. Component tests opt in
    // to jsdom via a file-level pragma.
    environment: "node",
    // Test files live wherever they belong; this glob just keeps
    // .svelte-kit / node_modules out of the picture.
    include: ["src/**/*.test.ts"],
    // Bail after the first failure when running locally to make
    // diagnosis easier. CI overrides via `vitest run --no-bail`.
    bail: 0,
    // Same timeout shape as cargo test — fail fast on hung tests.
    testTimeout: 10_000,
  },
});
