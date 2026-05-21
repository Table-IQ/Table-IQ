// SSH tunnel config + pure helpers (mirror the Rust serde shapes in src-tauri/src/ssh).

export type SshAuthMode = "password" | "keyfile" | "agent";

/** Tagged-union auth, matching Rust `SshAuth` (`#[serde(tag = "mode")]`). */
export type SshAuth =
  | { mode: "password" }
  | { mode: "keyfile"; path: string; passphrase?: string | null }
  | { mode: "agent" };

export interface SshConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  auth: SshAuth;
}

/** A fresh, disabled SSH config (agent auth, port 22). */
export function defaultSshConfig(): SshConfig {
  return { enabled: false, host: "", port: 22, username: "", auth: { mode: "agent" } };
}

/**
 * Whether an SSH config is usable. A disabled tunnel is always valid (it is ignored).
 * When enabled it needs host, username, an in-range port, and — for keyfile auth — a key path.
 */
export function sshConfigValid(s: SshConfig): boolean {
  if (!s.enabled) return true;
  if (!s.host.trim() || !s.username.trim()) return false;
  if (!Number.isInteger(s.port) || s.port < 1 || s.port > 65535) return false;
  if (s.auth.mode === "keyfile" && !s.auth.path.trim()) return false;
  return true;
}
