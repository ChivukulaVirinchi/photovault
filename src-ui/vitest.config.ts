import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Vitest config. Kept separate from `vite.config.ts` so the dev/build
// pipeline doesn't pull in any test-only plugins. Vitest reads this
// automatically when run from the `src-ui` directory.
//
// Why the svelte plugin? Some of our stores use the Svelte 5 `$state`
// rune; importing them in a `.test.ts` triggers the rune transform.
// Without the svelte plugin the tests panic at import time.
//
// Conventions for tests under `src/`:
//   - Plain utility tests use `node` environment (default) — fast,
//     no DOM, no jsdom overhead.
//   - Component tests should set `// @vitest-environment jsdom` at
//     the top of the file (we don't enable jsdom globally because
//     it's slow to spin up and most of our tests don't need it).
//   - Test files end in `.test.ts` and live next to the code they
//     exercise.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    bail: 0,
    testTimeout: 10_000,
  },
});
