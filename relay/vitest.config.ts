import {
  defineWorkersConfig,
} from "@cloudflare/vitest-pool-workers/config"

export default defineWorkersConfig({
  test: {
    poolOptions: {
      workers: {
        // Each test file gets its own isolated Durable Object instance
        // so state cannot leak across files. Disabling the per-test
        // isolation avoids an internal assertion from the pool's
        // stacked-storage bookkeeping in the bundled workerd.
        isolatedStorage: false,
        singleWorker: true,
        wrangler: { configPath: "./wrangler.toml" },
        miniflare: {
          compatibilityFlags: ["nodejs_compat"],
        },
      },
    },
  },
})
