import { describe, it, expect } from "vitest";
import { defaultSshConfig, sshConfigValid, type SshConfig } from "./ssh";

describe("defaultSshConfig", () => {
  it("is disabled with port 22 and agent auth by default", () => {
    const c = defaultSshConfig();
    expect(c.enabled).toBe(false);
    expect(c.port).toBe(22);
    expect(c.auth.mode).toBe("agent");
    expect(c.host).toBe("");
    expect(c.username).toBe("");
  });
});

describe("sshConfigValid", () => {
  const base: SshConfig = {
    enabled: true,
    host: "bastion",
    port: 22,
    username: "deploy",
    auth: { mode: "agent" },
  };

  it("treats a disabled tunnel as valid (it is ignored)", () => {
    expect(sshConfigValid({ ...base, enabled: false, host: "" })).toBe(true);
  });

  it("requires host and username when enabled", () => {
    expect(sshConfigValid({ ...base, host: "" })).toBe(false);
    expect(sshConfigValid({ ...base, username: "" })).toBe(false);
    expect(sshConfigValid(base)).toBe(true);
  });

  it("requires a port in range when enabled", () => {
    expect(sshConfigValid({ ...base, port: 0 })).toBe(false);
    expect(sshConfigValid({ ...base, port: 70000 })).toBe(false);
    expect(sshConfigValid({ ...base, port: 2222 })).toBe(true);
  });

  it("requires a key path for keyfile auth", () => {
    expect(sshConfigValid({ ...base, auth: { mode: "keyfile", path: "" } })).toBe(false);
    expect(sshConfigValid({ ...base, auth: { mode: "keyfile", path: "~/.ssh/id_ed25519" } })).toBe(true);
  });

  it("accepts password and agent auth without a path", () => {
    expect(sshConfigValid({ ...base, auth: { mode: "password" } })).toBe(true);
    expect(sshConfigValid({ ...base, auth: { mode: "agent" } })).toBe(true);
  });
});
