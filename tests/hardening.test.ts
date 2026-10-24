import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";
import { invokeBackend } from "../src/lib/platform.ts";

test("browser account access fails without making a network request", async () => {
  const originalFetch = globalThis.fetch;
  let networkCalls = 0;
  globalThis.fetch = async () => { networkCalls++; throw new Error("Unexpected request"); };
  try {
    await assert.rejects(invokeBackend("list_accounts"), /only in the desktop app/);
    assert.equal(networkCalls, 0);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("the fork has no network dashboard, updater, or backup capability", () => {
  const config = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const capabilities = JSON.parse(readFileSync(new URL("../src-tauri/capabilities/default.json", import.meta.url), "utf8"));
  const commands = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.equal(config.identifier, "com.nunomacedo.codex-switcher-local");
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.equal(config.plugins?.updater, undefined);
  assert.equal(manifest.scripts.lan, undefined);
  assert.ok(!capabilities.permissions.some((permission: string) => permission.startsWith("updater:")));
  assert.ok(!/export_accounts|import_accounts_slim|pub mod web/.test(commands));
  assert.equal(existsSync(new URL("../src-tauri/src/bin/codex-web.rs", import.meta.url)), false);
});
