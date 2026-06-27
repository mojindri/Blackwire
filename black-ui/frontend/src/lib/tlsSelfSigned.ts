export interface TlsSelfSignedValues {
  serverName: string;
  days: string;
}

export function defaultTlsSelfSignedValues(serverName: string): TlsSelfSignedValues {
  const name = normalizeServerName(serverName) || "example.com";
  return {
    serverName: name,
    days: "365"
  };
}

export function buildTlsSelfSignedInput(values: TlsSelfSignedValues): { serverName: string; days: number } {
  return {
    serverName: normalizeServerName(values.serverName) || "example.com",
    days: normalizeDays(values.days)
  };
}

export function expectedTlsSelfSignedPaths(serverName: string): { certificateFile: string; keyFile: string } {
  const name = normalizeServerName(serverName) || "example.com";
  const fileBase = sanitizeFileName(name);
  return {
    certificateFile: `/etc/blackwire/certs/${fileBase}.crt`,
    keyFile: `/etc/blackwire/certs/${fileBase}.key`
  };
}

function normalizeServerName(value: string): string {
  return value.trim().replace(/^\[(.*)\]$/, "$1");
}

function normalizeDays(value: string): number {
  const parsed = Number.parseInt(value.trim(), 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    return 365;
  }
  return Math.min(parsed, 3650);
}

function sanitizeFileName(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "tls-self-signed";
}
