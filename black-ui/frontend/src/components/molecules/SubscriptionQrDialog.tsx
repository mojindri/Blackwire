import { AlertCircle, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { lazy, Suspense, useEffect, useState } from "react";
import { fetchSubscriptionContent, subscriptionQrPayload } from "../../lib/subscription";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";

const QRCodeSVG = lazy(() => import("qrcode.react").then((module) => ({ default: module.QRCodeSVG })));

type QrState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; content: string; bytes: number };

export function SubscriptionQrDialog({
  url,
  label,
  onClose
}: {
  url: string;
  label: string;
  onClose: () => void;
}) {
  const [state, setState] = useState<QrState>({ status: "loading" });

  useEffect(() => {
    let active = true;
    setState({ status: "loading" });
    void fetchSubscriptionContent(url).then((result) => {
      if (!active) return;
      if (!result.ok) {
        setState({ status: "error", message: result.message });
        return;
      }
      const payload = subscriptionQrPayload(result.content);
      setState(
        payload.ok
          ? { status: "ready", content: payload.content, bytes: payload.bytes }
          : { status: "error", message: payload.message }
      );
    });
    return () => {
      active = false;
    };
  }, [url]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  return (
    <div className="dialog-backdrop subscription-qr-backdrop" role="presentation">
      <section className="dialog-panel subscription-qr-dialog" role="dialog" aria-modal="true" aria-labelledby="subscription-qr-title">
        <div className="dialog-head">
          <div>
            <h3 id="subscription-qr-title">Scan subscription content</h3>
            <p>{label}</p>
          </div>
          <IconButton label="Close subscription QR code" onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        <div className="dialog-body subscription-qr-body" aria-live="polite">
          {state.status === "loading" ? (
            <div className="subscription-qr-status">
              <LoaderCircle size={24} className="spinner" />
              <strong>Preparing secure QR code…</strong>
              <span>Fetching the current managed subscription content.</span>
            </div>
          ) : null}
          {state.status === "error" ? (
            <div className="subscription-qr-error">
              <AlertCircle size={20} />
              <div>
                <strong>QR code unavailable</strong>
                <span>{state.message}</span>
              </div>
            </div>
          ) : null}
          {state.status === "ready" ? (
            <>
              <div className="subscription-qr-stage">
                <Suspense fallback={<LoaderCircle size={24} className="spinner" />}>
                  <QRCodeSVG
                    value={state.content}
                    size={320}
                    level="L"
                    marginSize={4}
                    title={`Hiddify subscription content for ${label}`}
                  />
                </Suspense>
              </div>
              <div className="subscription-qr-instructions">
                <ShieldCheck size={18} />
                <div>
                  <strong>Open Hiddify, tap +, then Scan QR code</strong>
                  <span>This QR contains the fetched subscription content—not the panel URL. Treat it like a credential.</span>
                </div>
              </div>
              <span className="subscription-qr-size">{state.bytes.toLocaleString()} byte payload</span>
            </>
          ) : null}
        </div>
        <div className="dialog-actions">
          <Button type="button" variant="primary" onClick={onClose}>Done</Button>
        </div>
      </section>
    </div>
  );
}
