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

export async function fetchSubscriptionContent(url: string): Promise<{ ok: boolean; content: string; message: string }> {
  if (!url) return { ok: false, content: "", message: "Nothing to copy" };

  try {
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) {
      return { ok: false, content: "", message: `Subscription returned ${response.status}` };
    }
    const content = await response.text();
    if (!content.trim()) {
      return { ok: false, content: "", message: "Subscription is empty" };
    }

    return { ok: true, content, message: "Copied" };
  } catch {
    return { ok: false, content: "", message: "Subscription fetch failed" };
  }
}

export async function copySubscriptionContent(url: string): Promise<CopyResult> {
  const subscription = await fetchSubscriptionContent(url);
  return subscription.ok ? copyText(subscription.content) : subscription;
}

export function subscriptionQrPayload(content: string): SubscriptionQrPayload {
  const normalized = content.trim();
  if (!normalized) return { ok: false, message: "Subscription is empty" };

  const bytes = new TextEncoder().encode(normalized).byteLength;
  if (bytes > MAX_SUBSCRIPTION_QR_BYTES) {
    return {
      ok: false,
      message: `Subscription content is ${bytes.toLocaleString()} bytes. Keep it under ${MAX_SUBSCRIPTION_QR_BYTES.toLocaleString()} bytes for a reliably scannable QR code.`
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
