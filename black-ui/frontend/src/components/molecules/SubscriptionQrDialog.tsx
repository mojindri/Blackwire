import { AlertCircle, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { lazy, Suspense, useEffect, useMemo } from "react";
import { subscriptionQrPayload } from "../../lib/subscription";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";

const QRCodeSVG = lazy(() => import("qrcode.react").then((module) => ({ default: module.QRCodeSVG })));

export function SubscriptionQrDialog({
  url,
  label,
  onClose
}: {
  url: string;
  label: string;
  onClose: () => void;
}) {
  const payload = useMemo(() => subscriptionQrPayload(url), [url]);

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
            <h3 id="subscription-qr-title">Scan subscription URL</h3>
            <p>{label}</p>
          </div>
          <IconButton label="Close subscription QR code" onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        <div className="dialog-body subscription-qr-body" aria-live="polite">
          {!payload.ok ? (
            <div className="subscription-qr-error">
              <AlertCircle size={20} />
              <div>
                <strong>QR code unavailable</strong>
                <span>{payload.message}</span>
              </div>
            </div>
          ) : null}
          {payload.ok ? (
            <>
              <div className="subscription-qr-stage">
                <Suspense fallback={<LoaderCircle size={24} className="spinner" />}>
                  <QRCodeSVG
                    value={payload.content}
                    size={320}
                    level="L"
                    marginSize={4}
                    title={`Hiddify subscription URL for ${label}`}
                  />
                </Suspense>
              </div>
              <div className="subscription-qr-instructions">
                <ShieldCheck size={18} />
                <div>
                  <strong>Open Hiddify, tap +, then Scan QR code</strong>
                  <span>This QR contains the public subscription URL. Hiddify will fetch the current profile from Blackwire.</span>
                </div>
              </div>
              <span className="subscription-qr-size">{payload.bytes.toLocaleString()} byte URL</span>
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
