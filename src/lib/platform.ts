import type { ImportAccountsSummary } from "../types";

export type FileSource = string;

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function invokeBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) throw new Error("Account access is available only in the desktop app.");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function openExternalUrl(url: string): Promise<void> {
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function pickAuthJsonFile(): Promise<FileSource | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
    title: "Select auth.json file",
  });
  return !selected || Array.isArray(selected) ? null : selected;
}

export async function importLegacyAccountsFile(): Promise<ImportAccountsSummary | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: false,
    title: "Import existing Codex Switcher accounts",
    filters: [{ name: "Account file", extensions: ["json"] }],
  });
  if (!selected || Array.isArray(selected)) return null;
  return invokeBackend<ImportAccountsSummary>("import_legacy_accounts_file", { path: selected });
}

export function describeFileSource(source: FileSource | null): string {
  return source ?? "No file selected";
}
