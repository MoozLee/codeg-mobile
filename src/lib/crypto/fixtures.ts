/**
 * Deterministic crypto fixtures shared with the Rust integration test
 * `src-tauri/tests/crypto_fixtures_interop.rs`. Parsed from the single
 * source-of-truth JSON at `src-tauri/tests/fixtures/crypto_vectors.json`.
 *
 * The shape uses snake_case keys because both sides read the same file.
 */

import { readFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

export interface FixtureVector {
  client_secret_hex: string
  client_pub_hex: string
  server_secret_hex: string
  server_pub_hex: string
  nonce_hex: string
  plaintext_utf8: string
  expected_frame_base64: string
}

const HERE = dirname(fileURLToPath(import.meta.url))
const FIXTURE_PATH = resolve(
  HERE,
  "../../../src-tauri/tests/fixtures/crypto_vectors.json"
)

function loadFixtures(): FixtureVector[] {
  const raw = readFileSync(FIXTURE_PATH, "utf8")
  const parsed = JSON.parse(raw)
  if (!Array.isArray(parsed)) {
    throw new Error("crypto_vectors.json: expected a JSON array")
  }
  return parsed as FixtureVector[]
}

export const FIXTURE_VECTORS: readonly FixtureVector[] =
  Object.freeze(loadFixtures())
