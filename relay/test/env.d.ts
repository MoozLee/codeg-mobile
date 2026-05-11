// Type augmentation so TS knows what `cloudflare:test` exports and what
// bindings the test pool injects as `env`.
import type { Env } from "../src/types"

declare module "cloudflare:test" {
  interface ProvidedEnv extends Env {}
}
