import { useCallback, useEffect, useMemo, useState } from "react";
import { api, clearToken, getToken, setToken } from "./lib/api";
import type { AppData, InboundInput, ManagedUser, OutboundInput, PageKey, Settings, UserInput } from "./lib/types";
import { AppShell } from "./components/templates/AppShell";
import { AuthPage } from "./pages/AuthPage";
import { DashboardPage } from "./pages/DashboardPage";
import { UsersPage } from "./pages/UsersPage";
import { InboundsPage } from "./pages/InboundsPage";
import { OutboundsPage } from "./pages/OutboundsPage";
import { SectionsPage } from "./pages/SectionsPage";
import { ConfigPage } from "./pages/ConfigPage";
import { ServicePage } from "./pages/ServicePage";
import { SettingsPage } from "./pages/SettingsPage";

const emptyData: AppData = {
  status: null,
  settings: null,
  inbounds: [],
  outbounds: [],
  sections: [],
  users: [],
  traffic: { users: [], inbounds: [] },
  capabilities: null,
  service: null
};

export default function App() {
  const [token, setTokenState] = useState(getToken());
  const [page, setPage] = useState<PageKey>("users");
  const [data, setData] = useState<AppData>(emptyData);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const statusKnown = data.status !== null;
  const setupRequired = data.status?.setupRequired ?? false;
  const authenticated = Boolean(token) && !setupRequired;

  const refresh = useCallback(async () => {
    const status = await api.status();
    if (status.setupRequired) {
      setData((current) => ({ ...current, status }));
      return;
    }
    try {
      await api.me();
      if (!getToken()) {
        setToken();
        setTokenState("cookie");
      }
    } catch {
      clearToken();
      setTokenState("");
      setData((current) => ({ ...current, status }));
      return;
    }
    const [settings, inbounds, outbounds, sections, users, traffic, capabilities, service] = await Promise.all([
      api.settings(),
      api.inbounds(),
      api.outbounds(),
      api.sections(),
      api.users(),
      api.traffic().catch(() => ({ users: [], inbounds: [] })),
      api.capabilities(),
      api.serviceStatus().catch(() => null)
    ]);
    setData({ status, settings, inbounds, outbounds, sections, users, traffic, capabilities, service });
  }, []);

  useEffect(() => {
    refresh().catch((err: Error) => {
      setError(err.message);
      if (err.message.includes("authentication")) {
        clearToken();
        setTokenState("");
      }
    });
  }, [refresh, token]);

  const run = useCallback(
    async (action: () => Promise<unknown>, success: string, pending = "Working...") => {
      setBusy(true);
      setError("");
      setMessage(pending);
      try {
        const result = await action();
        const resultMessage = typeof result === "object" && result && "message" in result ? String(result.message) : success;
        setMessage(resultMessage);
        await refresh();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(false);
      }
    },
    [refresh]
  );

  const login = (username: string, password: string) => {
    run(async () => {
      const res = setupRequired ? await api.setup(username, password) : await api.login(username, password);
      setToken();
      setTokenState("cookie");
      return { message: `Logged in as ${res.username}` };
    }, "Logged in").catch(() => undefined);
  };

  const logout = () => {
    api.logout().catch(() => undefined);
    clearToken();
    setTokenState("");
    setData((current) => ({ ...current, settings: null, inbounds: [], outbounds: [], sections: [], users: [] }));
  };

  const actions = useMemo(
    () => ({
      saveUser: (id: number | null, input: UserInput) =>
        run(() => (id ? api.updateUser(id, input) : api.createUser(input)), "User saved", id ? "Saving user..." : "Creating user..."),
      toggleUser: (user: ManagedUser) =>
        run(
          () => (user.enabled ? api.disableUser(user.id) : api.enableUser(user.id)),
          user.enabled ? "User disabled" : "User enabled",
          user.enabled ? "Disabling user..." : "Enabling user..."
        ),
      deleteUser: (user: ManagedUser) => {
        if (window.confirm(`Delete ${user.email}?`)) run(() => api.deleteUser(user.id), "User deleted", "Deleting user...");
      },
      resetUsage: (user: ManagedUser) => run(() => api.resetUsage(user.id), "Usage reset", "Resetting usage..."),
      rotateUuid: (user: ManagedUser) => run(() => api.rotateUuid(user.id), "UUID rotated", "Rotating UUID..."),
      rotateToken: (user: ManagedUser) => run(() => api.rotateSubToken(user.id), "Subscription token rotated", "Rotating token..."),
      bulk: (ids: number[], action: string) => {
        if (ids.length === 0) return setMessage("Select at least one user first.");
        return run(() => api.bulkUsers({ userIds: ids, action }), "Bulk action applied", "Applying bulk action...");
      },
      createInbound: (input: InboundInput) => run(() => api.createInbound(input), "Inbound saved", "Creating inbound..."),
      updateInbound: (id: number, input: InboundInput) => run(() => api.updateInbound(id, input), "Inbound saved", "Saving inbound..."),
      deleteInbound: (id: number) => run(() => api.deleteInbound(id), "Inbound deleted", "Deleting inbound..."),
      createOutbound: (input: OutboundInput) => run(() => api.createOutbound(input), "Outbound saved", "Creating outbound..."),
      updateOutbound: (id: number, input: OutboundInput) => run(() => api.updateOutbound(id, input), "Outbound saved", "Saving outbound..."),
      deleteOutbound: (id: number) => run(() => api.deleteOutbound(id), "Outbound deleted", "Deleting outbound..."),
      saveSection: (name: string, enabled: boolean, value: string) =>
        run(() => api.updateSection(name, { enabled, value }), "Advanced config saved", "Saving advanced config..."),
      importConfig: (value: unknown) => run(() => api.configImport(value), "Config imported", "Importing config..."),
      restartBlackwire: () => run(api.serviceRestartBlackwire, "Blackwire restarted", "Restarting Blackwire..."),
      saveSettings: (settings: Settings) => run(() => api.updateSettings(settings), "Settings saved", "Saving settings..."),
      uuid: async () => (await api.uuid()).uuid
    }),
    [run]
  );

  if (!authenticated) {
    return <AuthPage setupRequired={setupRequired} checking={!statusKnown} error={error} onSubmit={login} />;
  }

  return (
    <AppShell
      page={page}
      status={data.status}
      message={error || message}
      busy={busy}
      onPage={setPage}
      onRefresh={() => run(refresh, "Refreshed", "Refreshing...")}
      onApply={() => run(api.configApply, "Config applied", "Applying config...")}
      onLogout={logout}
    >
      {page === "dashboard" ? <DashboardPage data={data} /> : null}
      {page === "users" ? (
        <UsersPage
          data={data}
          busy={busy}
          onSave={actions.saveUser}
          onUuid={actions.uuid}
          onToggle={actions.toggleUser}
          onDelete={actions.deleteUser}
          onReset={actions.resetUsage}
          onRotateUuid={actions.rotateUuid}
          onRotateToken={actions.rotateToken}
          onBulk={actions.bulk}
        />
      ) : null}
      {page === "inbounds" ? (
        <InboundsPage
          inbounds={data.inbounds}
          capabilities={data.capabilities}
          busy={busy}
          onCreate={actions.createInbound}
          onUpdate={actions.updateInbound}
          onDelete={actions.deleteInbound}
        />
      ) : null}
      {page === "outbounds" ? (
        <OutboundsPage
          outbounds={data.outbounds}
          capabilities={data.capabilities}
          busy={busy}
          onCreate={actions.createOutbound}
          onUpdate={actions.updateOutbound}
          onDelete={actions.deleteOutbound}
        />
      ) : null}
      {page === "sections" ? (
        <SectionsPage
          sections={data.sections}
          capabilities={data.capabilities}
          outbounds={data.outbounds}
          busy={busy}
          onSave={actions.saveSection}
        />
      ) : null}
      {page === "config" ? (
        <ConfigPage
          busy={busy}
          onValidate={() => run(api.configValidate, "Config valid", "Validating config...")}
          onWrite={() => run(api.configWrite, "Config written", "Writing config...")}
          onApply={() => run(api.configApply, "Config applied", "Applying config...")}
          onImport={actions.importConfig}
        />
      ) : null}
      {page === "service" ? <ServicePage service={data.service} busy={busy} onRestart={actions.restartBlackwire} /> : null}
      {page === "settings" ? <SettingsPage settings={data.settings} busy={busy} onSave={actions.saveSettings} /> : null}
    </AppShell>
  );
}
