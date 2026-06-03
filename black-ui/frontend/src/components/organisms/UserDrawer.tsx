import { AlertCircle, Copy, KeyRound, RotateCcw, Save, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Inbound, ManagedUser, Settings, UserInput } from "../../lib/types";
import { copyText } from "../../lib/clipboard";
import { formatBytes } from "../../lib/format";
import { subscriptionUrl } from "../../lib/subscription";
import {
  activeUserProtocol,
  buildUserInput,
  createUserEditorState,
  protocolLabel,
  replaceCredentialJson,
  syncCredentialFromFields,
  validateUserState,
  type UserEditorState
} from "../../lib/userConfigurator";
import { Button } from "../atoms/Button";
import { IconButton } from "../atoms/IconButton";
import { Input, Select, Textarea } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

export function UserDrawer({
  open,
  user,
  inbounds,
  settings,
  onClose,
  onSubmit,
  onUuid,
  onRotateUuid,
  onRotateToken,
  onReset,
  busy
}: {
  open: boolean;
  user: ManagedUser | null;
  inbounds: Inbound[];
  settings: Settings | null;
  onClose: () => void;
  onSubmit: (id: number | null, input: UserInput) => void;
  onUuid: () => Promise<string>;
  onRotateUuid: (user: ManagedUser) => void;
  onRotateToken: (user: ManagedUser) => void;
  onReset: (user: ManagedUser) => void;
  busy: boolean;
}) {
  const [state, setState] = useState<UserEditorState>(() => createUserEditorState(user, inbounds));
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [copyFeedback, setCopyFeedback] = useState("");

  useEffect(() => {
    setState(createUserEditorState(user, inbounds));
    setAdvancedOpen(false);
    setCopyFeedback("");
  }, [inbounds, user]);

  const protocol = useMemo(() => activeUserProtocol(inbounds, state.inboundId), [inbounds, state.inboundId]);
  const selectedInbound = useMemo(() => inbounds.find((item) => item.id === state.inboundId) ?? null, [inbounds, state.inboundId]);
  const validationIssues = useMemo(() => validateUserState(state, protocol, inbounds), [inbounds, protocol, state]);
  const issuesByField = useMemo(
    () => new Map(validationIssues.map((issue) => [issue.field, issue.message])),
    [validationIssues]
  );
  const subUrl = useMemo(() => subscriptionUrl(settings, user?.subToken ?? ""), [settings, user]);
  const saveDisabled = busy || inbounds.length === 0 || validationIssues.length > 0;

  if (!open) return null;

  const updateStructured = (patch: Partial<UserEditorState>) => {
    setState((current) => {
      const next = { ...current, ...patch };
      return syncCredentialFromFields(next, activeUserProtocol(inbounds, next.inboundId));
    });
  };

  const submit = () => {
    if (saveDisabled) return;
    onSubmit(user?.id ?? null, buildUserInput(state, protocol));
  };

  const copySubscription = async () => {
    const result = await copyText(subUrl);
    setCopyFeedback(result.ok ? "Copied" : "Copy failed. Select the URL and copy manually.");
    window.setTimeout(() => setCopyFeedback(""), 2600);
  };

  const summaryTitle = state.email.trim() || "New user";
  const summarySubtitle = selectedInbound ? `${selectedInbound.tag} :${selectedInbound.port}` : "No inbound selected";

  return (
    <aside className="drawer drawer-wide">
      <div className="drawer-head">
        <div>
          <h2>{summaryTitle}</h2>
          <p>{user ? "Edit one managed credential with inbound-aware access fields." : "Create a managed credential from the selected inbound."}</p>
        </div>
        <IconButton label="Close" onClick={onClose}>
          <X size={18} />
        </IconButton>
      </div>

      <div className="drawer-body drawer-body-configurator">
        <section className="drawer-card drawer-summary-card">
          <div className="summary-head">
            <div>
              <strong>{summaryTitle}</strong>
              <span>{summarySubtitle}</span>
            </div>
            <Switch checked={state.enabled} onChange={(enabled) => updateStructured({ enabled })} label={state.enabled ? "Enabled" : "Disabled"} />
          </div>
          <div className="summary-badges">
            <span className="summary-chip">{protocolLabel(protocol)}</span>
            {selectedInbound ? <span className="summary-chip">{selectedInbound.tag}</span> : null}
            <span className="summary-chip summary-chip-soft">{state.trafficLimitGB.trim() ? `${state.trafficLimitGB.trim()} GB cap` : "Unlimited quota"}</span>
          </div>
        </section>

        <section className="drawer-card configurator-section">
          <div className="section-editor-head">
            <div>
              <h3>Identity</h3>
              <p>Pick the inbound first. The dialog shapes the access fields from that protocol.</p>
            </div>
          </div>
          <div className="configurator-grid">
            <Field label="Email">
              <Input value={state.email} onChange={(e) => updateStructured({ email: e.target.value })} placeholder="alice@example.com" />
            </Field>
            <Field label="Inbound">
              <Select value={state.inboundId} onChange={(e) => updateStructured({ inboundId: Number(e.target.value) })} disabled={inbounds.length === 0}>
                {inbounds.length === 0 ? <option value={0}>No inbounds available</option> : null}
                {inbounds.map((inbound) => (
                  <option key={inbound.id} value={inbound.id}>
                    {inbound.tag} :{inbound.port} ({inbound.protocol})
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          {issuesByField.get("email") ? <div className="field-error">{issuesByField.get("email")}</div> : null}
          {issuesByField.get("inboundId") ? <div className="field-error">{issuesByField.get("inboundId")}</div> : null}
        </section>

        <section className="drawer-card configurator-section">
          <div className="section-editor-head">
            <div>
              <h3>Access</h3>
              <p>These fields come from the selected inbound&apos;s protocol, with advanced JSON available only as a fallback.</p>
            </div>
          </div>
          <div className="configurator-grid">
            <Field label="UUID" hint="Blackwire still uses this as the core identity and fallback secret.">
              <div className="inline-field">
                <Input value={state.uuid} onChange={(e) => updateStructured({ uuid: e.target.value })} placeholder="2c22c0f6-c084-482f-b2ce-129fa1fd8255" />
                <IconButton label="Generate UUID" onClick={async () => updateStructured({ uuid: await onUuid() })} disabled={busy}>
                  <KeyRound size={17} />
                </IconButton>
              </div>
            </Field>

            {protocol === "vless" ? (
              <Field label="Flow" hint="Leave empty for ordinary VLESS clients.">
                <Input value={state.flow} onChange={(e) => updateStructured({ flow: e.target.value })} placeholder="xtls-rprx-vision" />
              </Field>
            ) : (
              <div />
            )}

            {protocol === "trojan" || protocol === "shadowsocks" ? (
              <Field label="Password">
                <Input value={state.password} onChange={(e) => updateStructured({ password: e.target.value })} placeholder="shared-secret" />
              </Field>
            ) : null}

            {protocol === "hysteria2" ? (
              <Field label="Auth">
                <Input value={state.auth} onChange={(e) => updateStructured({ auth: e.target.value })} placeholder="hy2-auth-token" />
              </Field>
            ) : null}

            {protocol === "shadowsocks" ? (
              <Field label="Method" hint="Optional per-user override. Leave empty to inherit from the inbound.">
                <Input value={state.method} onChange={(e) => updateStructured({ method: e.target.value })} placeholder="2022-blake3-aes-128-gcm" />
              </Field>
            ) : null}
          </div>
          {protocol === "vmess" ? <p className="field-hint">VMess users only need the shared UUID here. No extra credential keys are required.</p> : null}
          {protocol === "unknown" ? <p className="field-hint">This inbound protocol does not have a structured user form yet. Use Advanced only if you truly need custom credential keys.</p> : null}
          {issuesByField.get("uuid") ? <div className="field-error">{issuesByField.get("uuid")}</div> : null}
          {issuesByField.get("password") ? <div className="field-error">{issuesByField.get("password")}</div> : null}
          {issuesByField.get("auth") ? <div className="field-error">{issuesByField.get("auth")}</div> : null}
        </section>

        <section className="drawer-card configurator-section">
          <div className="section-editor-head">
            <div>
              <h3>Limits</h3>
              <p>Quota is edited in GB here and converted to bytes when we save.</p>
            </div>
          </div>
          <div className="configurator-grid">
            <Field label="Traffic limit (GB)" hint="Leave empty for unlimited access.">
              <Input
                type="number"
                min={0}
                step="0.25"
                value={state.trafficLimitGB}
                onChange={(e) => updateStructured({ trafficLimitGB: e.target.value })}
                placeholder="10"
              />
            </Field>
            <Field label="Expiry">
              <Input type="datetime-local" value={state.expiryLocal} onChange={(e) => updateStructured({ expiryLocal: e.target.value })} />
            </Field>
          </div>
          {issuesByField.get("trafficLimitGB") ? <div className="field-error">{issuesByField.get("trafficLimitGB")}</div> : null}
        </section>

        <section className="drawer-card configurator-section">
          <div className="section-editor-head">
            <div>
              <h3>Notes</h3>
              <p>Keep any local operator context here instead of overloading the email or credential fields.</p>
            </div>
          </div>
          <Field label="Note">
            <Textarea rows={3} value={state.note} onChange={(e) => updateStructured({ note: e.target.value })} placeholder="VIP user, office gateway, temporary access..." />
          </Field>
        </section>

        <section className="drawer-card configurator-section">
          <div className="section-editor-head">
            <div>
              <h3>Advanced</h3>
              <p>Use this only for custom credential keys that do not have a first-class field yet.</p>
            </div>
            <Button variant="ghost" onClick={() => setAdvancedOpen((current) => !current)} type="button">
              {advancedOpen ? "Hide JSON" : "Show JSON"}
            </Button>
          </div>
          {advancedOpen ? (
            <Field label="Credential JSON" hint='Examples: {"password":"..."} for Trojan or {"auth":"..."} for Hysteria2. Unknown keys are preserved.'>
              <div className="advanced-slice">
                <Textarea rows={7} value={state.credentialText} onChange={(e) => setState((current) => replaceCredentialJson(current, e.target.value))} />
                {state.credentialError ? <div className="field-error">{state.credentialError}</div> : null}
              </div>
            </Field>
          ) : (
            <p className="field-hint">Advanced credential JSON is collapsed by default so the common path stays clean.</p>
          )}
        </section>

        {user ? (
          <>
            <section className="drawer-card configurator-section">
              <div className="section-editor-head">
                <div>
                  <h3>Usage & Actions</h3>
                  <p>Operational actions stay separate from the edit fields so this drawer is easier to scan.</p>
                </div>
              </div>
              <div className="summary-badges">
                <span className="summary-chip">Upload {formatBytes(user.uploadBytes)}</span>
                <span className="summary-chip">Download {formatBytes(user.downloadBytes)}</span>
                <span className="summary-chip summary-chip-soft">Total {formatBytes(user.uploadBytes + user.downloadBytes)}</span>
              </div>
              <div className="drawer-actions">
                <Button variant="secondary" icon={<RotateCcw size={16} />} onClick={() => onReset(user)} loading={busy}>
                  Reset usage
                </Button>
                <Button variant="secondary" icon={<KeyRound size={16} />} onClick={() => onRotateUuid(user)} loading={busy}>
                  Rotate UUID
                </Button>
                <Button variant="secondary" icon={<KeyRound size={16} />} onClick={() => onRotateToken(user)} loading={busy}>
                  Rotate token
                </Button>
              </div>
            </section>

            <section className="drawer-card configurator-section">
              <div className="section-editor-head">
                <div>
                  <h3>Subscription</h3>
                  <p>Copy the managed subscription link without mixing it into the editable access fields.</p>
                </div>
              </div>
              <div className="copy-row">
                <Input value={subUrl} readOnly />
                <IconButton label="Copy subscription URL" onClick={copySubscription} disabled={!subUrl}>
                  <Copy size={16} />
                </IconButton>
              </div>
              {copyFeedback ? (
                <div className={copyFeedback === "Copied" ? "copy-feedback" : "copy-feedback copy-feedback-error"} aria-live="polite">
                  {copyFeedback}
                </div>
              ) : null}
            </section>
          </>
        ) : null}

        {inbounds.length === 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>Create an inbound before adding users.</span>
          </div>
        ) : null}
        {validationIssues.length > 0 ? (
          <div className="error-line inline-error">
            <AlertCircle size={15} />
            <span>{validationIssues[0].message}</span>
          </div>
        ) : null}
      </div>

      <div className="drawer-foot">
        <Button variant="ghost" onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button variant="primary" icon={<Save size={16} />} onClick={submit} loading={busy} disabled={saveDisabled}>
          Save User
        </Button>
      </div>
    </aside>
  );
}
