import type { Settings } from "./types";
import { copyText } from "./clipboard";

const LOCAL_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);
export const MAX_SUBSCRIPTION_QR_BYTES = 1800;
type CopyResult = { ok: boolean; message: string };
export type SubscriptionQrPayload =
  | { ok: true; content: string; bytes: number }
  | { ok: false; message: string };

export function subscriptionUrl(settings: Settings | null, token: string): string {
  if (!settings || !token) return "";
  return `${subscriptionBaseUrl(settings)}/sub/${token}`;
}

export async function copySubscriptionUrl(url: string): Promise<CopyResult> {
  return copyText(url);
}

export function subscriptionQrPayload(url: string): SubscriptionQrPayload {
  const normalized = url.trim();
  if (!normalized) return { ok: false, message: "Subscription URL is empty" };

  try {
    const parsed = new URL(normalized);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return { ok: false, message: "Subscription URL must use HTTP or HTTPS" };
    }
  } catch {
    return { ok: false, message: "Subscription URL is invalid" };
  }

  const bytes = new TextEncoder().encode(normalized).byteLength;
  if (bytes > MAX_SUBSCRIPTION_QR_BYTES) {
    return {
      ok: false,
      message: `Subscription URL is ${bytes.toLocaleString()} bytes. Keep it under ${MAX_SUBSCRIPTION_QR_BYTES.toLocaleString()} bytes for a reliably scannable QR code.`
    };
  }
  return { ok: true, content: normalized, bytes };
}

function subscriptionBaseUrl(settings: Settings): string {
  const configured = settings.publicBaseUrl.trim();
  if (!configured) return currentOrigin();

  try {
    const url = new URL(configured);
    const current = currentOrigin();
    if (current) {
      const currentUrl = new URL(current);
      if (LOCAL_HOSTS.has(url.hostname) && !LOCAL_HOSTS.has(currentUrl.hostname)) {
        return currentUrl.origin;
      }
    }
    return trimTrailingSlash(url.toString());
  } catch {
    return trimTrailingSlash(configured);
  }
}

function currentOrigin(): string {
  if (typeof window === "undefined") return "";
  return window.location.origin;
}

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}
