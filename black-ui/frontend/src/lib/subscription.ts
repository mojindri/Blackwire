import type { Settings } from "./types";
import { copyText } from "./clipboard";

const LOCAL_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);
type CopyResult = { ok: boolean; message: string };

export function subscriptionUrl(settings: Settings | null, token: string): string {
  if (!settings || !token) return "";
  return `${subscriptionBaseUrl(settings)}/sub/${token}`;
}

export async function fetchSubscriptionContent(url: string): Promise<{ ok: boolean; content: string; message: string }> {
  if (!url) return { ok: false, content: "", message: "Nothing to copy" };

  try {
    const response = await fetch(url.endsWith("/raw") ? url : `${url}/raw`, { cache: "no-store" });
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
