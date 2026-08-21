import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

// Map the cloudflare:workers virtual module to a local stub for tests. The real
// module only exists inside the Workers runtime.
export default defineConfig({
  test: {
    alias: {
      'cloudflare:workers': fileURLToPath(
        new URL('./tests/stubs/cloudflare-workers.ts', import.meta.url)
      ),
    },
  },
});
