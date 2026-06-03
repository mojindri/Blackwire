import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { chromium } from "playwright";

const args = process.argv.slice(2);
const argv = new Set(args);
const uiUrl = getArg(args, "--ui-url", args.find((value) => !value.startsWith("--")) ?? "http://127.0.0.1:18180");
const configPathArg = getArg(args, "--config-path", "");
const grpcAddressArg = getArg(args, "--grpc", "");
const publicBaseUrlArg = getArg(args, "--public-base-url", "");
const subscriptionHostArg = getArg(args, "--subscription-host", "");
const adminUser = getArg(args, "--admin-user", "admin");
const adminPassword = getArg(args, "--admin-pass", "password123");
const headed = argv.has("--headed");
const restoreSettings = argv.has("--restore-settings");
const keepSettings = argv.has("--keep-settings");

const qaRunId = `qa-${Date.now().toString(36)}`;
const defaultConfigPath = path.join(tmpdir(), "black-ui-qa-inbounds", `${qaRunId}-config.json`);
const runConfigPath = configPathArg || defaultConfigPath;
const screenshotPath = path.join(path.dirname(runConfigPath), `qa-results-${qaRunId}.json`);

const basePort = 28700;
const cases = [
  {
    id: "vless-tcp",
    name: "VLESS/TCP",
    protocol: "vless",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 1,
    protocolValues: { decryption: "none" },
    expected: {
      protocol: "vless",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vmess-tcp",
    name: "VMess/TCP",
    protocol: "vmess",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 2,
    expected: {
      protocol: "vmess",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" },
      settings: { clients: [] }
    }
  },
  {
    id: "trojan-tcp",
    name: "Trojan/TCP",
    protocol: "trojan",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 3,
    expected: {
      protocol: "trojan",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "shadowsocks-tcp",
    name: "Shadowsocks/TCP",
    protocol: "shadowsocks",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 4,
    protocolValues: { method: "2022-blake3-aes-128-gcm" },
    expected: {
      protocol: "shadowsocks",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" },
      settings: { method: "2022-blake3-aes-128-gcm" }
    }
  },
  {
    id: "hysteria2-tcp",
    name: "Hysteria2/TCP",
    protocol: "hysteria2",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 5,
    expected: {
      protocol: "hysteria2",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "vless-ws",
    name: "VLESS/WS",
    protocol: "vless",
    network: "ws",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 6,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/ws", host: "ws.example.com" },
    expected: {
      protocol: "vless",
      transport: "ws",
      streamSettings: {
        network: "ws",
        security: "none",
        wsSettings: { path: "/ws", headers: { Host: "ws.example.com" } }
      },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-grpc",
    name: "VLESS/gRPC",
    protocol: "vless",
    network: "grpc",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 7,
    protocolValues: { decryption: "none" },
    transportValues: { serviceName: "grpc-service" },
    expected: {
      protocol: "vless",
      transport: "grpc",
      streamSettings: { network: "grpc", security: "none", grpcSettings: { serviceName: "grpc-service" } },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-httpupgrade",
    name: "VLESS/HTTPUpgrade",
    protocol: "vless",
    network: "httpupgrade",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 8,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/upgrade", host: "upgrade.example.com" },
    expected: {
      protocol: "vless",
      transport: "httpupgrade",
      streamSettings: {
        network: "httpupgrade",
        security: "none",
        httpupgradeSettings: { path: "/upgrade", host: "upgrade.example.com" }
      },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-splithttp",
    name: "VLESS/SplitHTTP",
    protocol: "vless",
    network: "splithttp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 9,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/packet" },
    expected: {
      protocol: "vless",
      transport: "splithttp",
      streamSettings: {
        network: "splithttp",
        security: "none",
        splithttpSettings: { path: "/packet" }
      },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-kcp",
    name: "VLESS/KCP",
    protocol: "vless",
    network: "kcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 10,
    protocolValues: { decryption: "none" },
    expected: {
      protocol: "vless",
      transport: "kcp",
      streamSettings: { network: "kcp", security: "none" },
      settings: { decryption: "none" }
    },
    optional: true
  },
  {
    id: "vless-quic",
    name: "VLESS/QUIC",
    protocol: "vless",
    network: "quic",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 11,
    protocolValues: { decryption: "none" },
    expected: {
      protocol: "vless",
      transport: "quic",
      streamSettings: { network: "quic", security: "none" },
      settings: { decryption: "none" }
    },
    optional: true
  },
  {
    id: "vless-tls",
    name: "TCP+TLS",
    protocol: "vless",
    network: "tcp",
    security: "tls",
    listen: "127.0.0.1",
    port: basePort + 12,
    protocolValues: { decryption: "none" },
    securityValues: {
      serverName: "example.com",
      alpn: "h2,http/1.1",
      certificateFile: "/etc/blackwire/fullchain.pem",
      keyFile: "/etc/blackwire/privkey.pem"
    },
    expected: {
      protocol: "vless",
      transport: "tcp",
      streamSettings: {
        network: "tcp",
        security: "tls",
        tlsSettings: {
          serverName: "example.com",
          alpn: ["h2", "http/1.1"],
          certificateFile: "/etc/blackwire/fullchain.pem",
          keyFile: "/etc/blackwire/privkey.pem"
        }
      },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-ws-tls",
    name: "VLESS/WS+TLS",
    protocol: "vless",
    network: "ws",
    security: "tls",
    listen: "127.0.0.1",
    port: basePort + 13,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/tls", host: "tls.example.com" },
    securityValues: {
      serverName: "tls.example.com",
      alpn: "h2,http/1.1",
      certificateFile: "/etc/blackwire/fullchain.pem",
      keyFile: "/etc/blackwire/privkey.pem"
    },
    expected: {
      protocol: "vless",
      transport: "ws",
      streamSettings: {
        network: "ws",
        security: "tls",
        wsSettings: { path: "/tls", headers: { Host: "tls.example.com" } },
        tlsSettings: {
          serverName: "tls.example.com",
          alpn: ["h2", "http/1.1"],
          certificateFile: "/etc/blackwire/fullchain.pem",
          keyFile: "/etc/blackwire/privkey.pem"
        }
      },
      settings: { decryption: "none" }
    }
  },
  {
    id: "vless-ws-reality-fail",
    name: "VLESS/WS+REALITY (expected guard)",
    protocol: "vless",
    network: "ws",
    security: "reality",
    listen: "127.0.0.1",
    port: basePort + 14,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/blocked" },
    expectFailure: true,
    expectedFailureHint: "REALITY currently works only with TCP in this editor.",
    expected: null,
    optional: false
  },
  {
    id: "vless-sniffing",
    name: "VLESS/Sniffing + Limits",
    protocol: "vless",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 15,
    protocolValues: { decryption: "none" },
    sniffing: {
      enabled: true,
      destOverride: ["http", "tls"],
      metadataOnly: true,
      routeOnly: false
    },
    limits: {
      maxConnections: "8000",
      maxHandshakeSeconds: "12"
    },
    expected: {
      protocol: "vless",
      transport: "tcp",
      streamSettings: { network: "tcp", security: "none" },
      settings: { decryption: "none" },
      sniffing: { enabled: true, destOverride: ["http", "tls"], metadataOnly: true },
      limits: { maxConnections: 8000, maxHandshakeSeconds: 12 }
    }
  },
  {
    id: "vless-advanced-unknown",
    name: "VLESS/Advanced unknown keys preserve",
    protocol: "vless",
    network: "ws",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 16,
    protocolValues: { decryption: "none" },
    transportValues: { path: "/adv", host: "adv.example.com" },
    advanced: {
      settings: {
        decryption: "none",
        _qaKeepSettings: { marker: "keep-me", nested: { ok: true } }
      },
      streamSettings: {
        network: "ws",
        security: "none",
        wsSettings: { path: "/adv", headers: { Host: "adv.example.com" } },
        _qaKeepTransport: { marker: "preserve", nested: { value: 1 } }
      }
    },
    expected: {
      protocol: "vless",
      transport: "ws",
      settings: {
        decryption: "none",
        _qaKeepSettings: { marker: "keep-me", nested: { ok: true } }
      },
      streamSettings: {
        network: "ws",
        security: "none",
        wsSettings: { path: "/adv", headers: { Host: "adv.example.com" } },
        _qaKeepTransport: { marker: "preserve", nested: { value: 1 } }
      }
    }
  }
];

const results = [];
const skippedCases = [];

await main();

async function main() {
  await mkdir(path.dirname(runConfigPath), { recursive: true });
  const browser = await launchBrowser();
  if (!browser) {
    throw new Error(
      "No Playwright browser available. Install once with `npx playwright install chromium` or run with a system browser channel."
    );
  }
  const context = await browser.newContext({ viewport: { width: 1440, height: 980 } });
  await primeAuthCookie(context);
  const page = await context.newPage();
  const consoleMessages = [];
  page.on("console", (msg) => {
    if (["error", "warning"].includes(msg.type())) {
      consoleMessages.push(`${msg.type()}: ${msg.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    consoleMessages.push(`pageerror: ${error.message}`);
  });

  try {
    page.setDefaultTimeout(12000);
    await page.goto(uiUrl, { waitUntil: "networkidle" });

    await ensureAuthenticated(page);

    await nav(page, "Users");
    await page.getByRole("heading", { name: "Users", exact: true }).waitFor();

    const originalSettings = await readSettingsFromUI(page);
    let workingConfigPath = runConfigPath;
    const updated = await applyQaSettings(page, {
      configPath: runConfigPath,
      grpcAddress: grpcAddressArg || originalSettings.grpcAddress,
      publicBaseUrl: publicBaseUrlArg || originalSettings.publicBaseUrl,
      subscriptionHost: subscriptionHostArg || originalSettings.subscriptionHost
    });
    workingConfigPath = updated.configPath;

    const createdTags = [];
    for (const testCase of cases) {
      const tag = `${qaRunId}-${testCase.id}`;
      try {
        await runCase(page, {
          ...testCase,
          tag
        }, workingConfigPath);
        createdTags.push(tag);
      } catch (error) {
        results.push({
          name: testCase.name,
          status: "FAILED",
          details: `${String(error)}`
        });
      }
    }

    await runDeleteProtectionChecks(page, workingConfigPath, createdTags);
    await runEditToggleEnabled(page, `${qaRunId}-${cases[0].id}`, workingConfigPath);

    if (restoreSettings && !keepSettings) {
      await restoreOriginalSettings(page, originalSettings);
    }

    await writeFile(
      screenshotPath,
      `${JSON.stringify(
        {
          generatedAt: new Date().toISOString(),
          configPath: runConfigPath,
          counts: {
            passed: results.filter((item) => item.status === "PASS").length,
            skipped: results.filter((item) => item.status === "SKIPPED").length,
            failed: results.filter((item) => item.status === "FAILED").length
          },
          results,
          skippedCases
        },
        null,
        2
      )}\n`,
      "utf8"
    );
    printSummary();

    const failed = results.filter((item) => item.status === "FAILED" || item.status === "SKIPPED");
    if (failed.length > 0) {
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  } finally {
    await context.close().catch(() => {});
    await browser.close().catch(() => {});
    const relevantConsole = consoleMessages.filter((item) => !item.includes("401"));
    if (relevantConsole.length > 0) {
      console.warn("Browser console noise:");
      relevantConsole.forEach((line) => console.warn(`  ${line}`));
    }
  }
}

async function runCase(page, testCase, configPath) {
  if (testCase.optional && ["kcp", "quic"].includes(testCase.network)) {
    results.push({
      name: testCase.name,
      status: "SKIPPED",
      details: `${testCase.network} transport not exposed by the current UI`
    });
    skippedCases.push({
      name: testCase.name,
      reason: `${testCase.network} transport not exposed by the current UI`
    });
    return;
  }
  if (testCase.optional && !(await isTransportAvailable(page, testCase.network))) {
    results.push({
      name: testCase.name,
      status: "SKIPPED",
      details: `${testCase.network} transport not exposed by current capabilities`
    });
    skippedCases.push({
      name: testCase.name,
      reason: `${testCase.network} transport not exposed by current capabilities`
    });
    return;
  }

  const { tag, protocol, network, security, listen, port, protocolValues = {}, transportValues = {}, securityValues = {} } = testCase;
  const expectation = { enabled: true, ...testCase.expected };
  const expectFailure = Boolean(testCase.expectFailure);
  const failureHint = testCase.expectedFailureHint || "expected guard/validation error";

  await openNewInbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", tag);
  await selectField(page, "Protocol", protocol);
  await fillField(page, "Listen host", listen);
  await fillField(page, "Port", String(port));

  await selectTab(page, "Protocol");
  await fillProtocolFields(page, protocol, protocolValues);

  await selectTab(page, "Transport");
  await selectField(page, "Network", network);
  await fillTransportFields(page, network, transportValues);

  await selectTab(page, "Security");
  await selectField(page, "Security layer", security);
  await fillSecurityFields(page, security, securityValues);

  if (testCase.sniffing || testCase.limits) {
    await selectTab(page, "Sniffing");
    await fillSniffing(page, testCase.sniffing || {}, testCase.limits || {});
  }

  if (testCase.advanced) {
    await selectTab(page, "Advanced");
    await fillAdvancedJSON(page, testCase.advanced.settings, testCase.advanced.streamSettings);
  }

  const saveButton = page.getByRole("button", { name: "Save Inbound", exact: true });
  const saveDisabled = await saveButton.isDisabled();
  if (expectFailure) {
    const hasFailure = await hasText(page, failureHint);
    if (!saveDisabled && !hasFailure) {
      await saveButton.click();
      try {
        await waitForTopMessage(page, /Inbound saved/i, 2500);
        await closeDrawer(page);
        results.push({
          name: testCase.name,
          status: "FAILED",
          details: "expected failure, but save completed"
        });
        return;
      } catch {
        // expected: save blocked
      }
    }

    const hasErrorLine = await hasText(page, "Fix invalid JSON in Advanced before saving.");
    const passed = hasFailure || hasErrorLine;
    await closeDrawer(page);
    results.push({
      name: testCase.name,
      status: passed ? "PASS" : "FAILED",
      details: passed ? "validation guard confirmed" : `expected hint "${failureHint}" but not found`
    });
    return;
  }

  if (saveDisabled) {
    const msg = `Save Inbound is disabled for ${tag}.`;
    await closeDrawer(page);
    throw new Error(msg);
  }

  await saveButton.click();
  await waitForConfigSaved(page);

  await nav(page, "Inbounds");
  await waitForInboundTag(page, tag);
  const inboundFromDisk = await readInboundFromDisk(configPath, tag);
  if (!inboundFromDisk) {
    throw new Error(`expected ${tag} to exist in ${configPath}`);
  }
  assertSubset(inboundFromDisk, expectation, `${tag}`);
  results.push({
    name: testCase.name,
    status: "PASS",
    details: `${tag} persisted to ${configPath}`
  });
}

async function runDeleteProtectionChecks(page, configPath, createdTags) {
  await nav(page, "Inbounds");
  const rowCount = await countInboundRows(page);
  if (rowCount === 0) throw new Error("inbounds list is empty; cannot run delete checks");
  if (rowCount === 1) {
    const row = page.locator("tbody tr").first();
    await row.getByRole("button").first().click();
    const deleteButton = page.getByRole("button", { name: "Delete", exact: true });
    const disabled = await deleteButton.isDisabled();
    const hint = await hasText(page, "Create another inbound before deleting this one.");
    await closeDrawer(page);
    results.push({
      name: "Delete guard (single inbound)",
      status: disabled && hint ? "PASS" : "FAILED",
      details: disabled ? "delete blocked with guard text" : "delete was available while only one inbound existed"
    });
  }

  const probeTag = `${qaRunId}-probe-delete`;
  await createInboundByDirectValues(page, {
    tag: probeTag,
    protocol: "vless",
    network: "tcp",
    security: "none",
    listen: "127.0.0.1",
    port: basePort + 18,
    protocolValues: { decryption: "none" },
    transportValues: {},
    securityValues: {}
  });
  await nav(page, "Inbounds");
  await page.getByRole("button", { name: probeTag, exact: true }).click();
  const deleteButton = page.getByRole("button", { name: "Delete", exact: true });
  await deleteButton.waitFor({ timeout: 10000 });
  await deleteButton.click();
  await waitForConfigSaved(page);
  await closeDrawer(page);
  await nav(page, "Inbounds");
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.waitForTimeout(1000);
  const removedOnUi = (await page.getByRole("button", { name: probeTag, exact: true }).count()) === 0;
  const probeOnDisk = await readInboundFromDisk(configPath, probeTag);
  results.push({
    name: "Delete flow",
    status: removedOnUi && !probeOnDisk ? "PASS" : "FAILED",
    details: removedOnUi && !probeOnDisk ? "probe inbound deleted and removed from config file" : "probe inbound deletion failed"
  });
}

async function runEditToggleEnabled(page, tag, configPath) {
  await openExistingInbound(page, tag);
  await selectTab(page, "Basic");
  const enabledSwitch = page.getByRole("switch", { name: "Enabled" });
  const checked = await enabledSwitch.getAttribute("aria-checked");
  if (checked === "true") {
    await enabledSwitch.click();
  }
  await page.getByRole("button", { name: "Save Inbound", exact: true }).click();
  await waitForConfigSaved(page);
  await nav(page, "Inbounds");
  await waitForInboundTag(page, tag);
  await closeDrawer(page).catch(() => {});
  const saved = await readInboundFromDisk(configPath, tag);
  results.push({
    name: "Edit + enabled toggle",
    status: !(await isInboundInConfig(configPath, tag)) ? "PASS" : "FAILED",
    details: !(await isInboundInConfig(configPath, tag))
      ? "disabled inbound omitted from generated config"
      : "disabled inbound still present in generated config"
  });
}

async function createInboundByDirectValues(page, def) {
  await openNewInbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", def.tag);
  await selectField(page, "Protocol", def.protocol);
  await fillField(page, "Listen host", def.listen);
  await fillField(page, "Port", String(def.port));

  await selectTab(page, "Protocol");
  await fillProtocolFields(page, def.protocol, def.protocolValues || {});

  await selectTab(page, "Transport");
  await selectField(page, "Network", def.network);
  await fillTransportFields(page, def.network, def.transportValues || {});

  await selectTab(page, "Security");
  await selectField(page, "Security layer", def.security);
  await fillSecurityFields(page, def.security, def.securityValues || {});

  const saveButton = page.getByRole("button", { name: "Save Inbound", exact: true });
  if (await saveButton.isDisabled()) {
    const status = await getTopMessageText(page);
    await closeDrawer(page);
    throw new Error(`probe inbound could not be created: ${status}`);
  }
  await saveButton.click();
  await waitForConfigSaved(page);
}

async function ensureAuthenticated(page) {
  const isCreateVisible = await page.getByRole("heading", { name: "Create admin", exact: true }).isVisible().catch(() => false);
  const isLoginVisible = await page.getByRole("heading", { name: "Panel login", exact: true }).isVisible().catch(() => false);

  if (isCreateVisible) {
    await fillField(page, "Username", adminUser);
    await fillField(page, "Password", adminPassword);
    await page.getByRole("button", { name: "Create and enter", exact: true }).click();
  } else if (isLoginVisible) {
    await fillField(page, "Username", adminUser);
    await fillField(page, "Password", adminPassword);
    await page.getByRole("button", { name: "Login", exact: true }).click();
  }

  await waitForAuthenticatedShell(page);
}

async function waitForAuthenticatedShell(page) {
  await page.waitForFunction(
    () => Boolean(document.querySelector("aside.sidebar")) || Array.from(document.querySelectorAll("h1")).some((node) => /Users|Inbounds|Outbounds|Settings|Advanced Config/i.test(node.textContent ?? "")),
    { timeout: 12000 }
  );
}

async function primeAuthCookie(context) {
  try {
    const db = new DatabaseSync(path.join("..", "data", "black-ui.db"));
    const row = db.prepare("SELECT token FROM sessions ORDER BY created_at DESC LIMIT 1").get();
    if (!row?.token) return;
    await context.addCookies([
      {
        name: "black_ui_session",
        value: row.token,
        url: "http://127.0.0.1:18180/"
      }
    ]);
  } catch {
    // If the DB is unavailable, fall back to the normal auth flow.
  }
}

async function readSettingsFromUI(page) {
  await nav(page, "Settings");
  return {
    configPath: await page.getByLabel("Config path", { exact: true }).inputValue(),
    grpcAddress: await page.getByLabel("gRPC address", { exact: true }).inputValue(),
    publicBaseUrl: await page.getByLabel("Public base URL", { exact: true }).inputValue(),
    subscriptionHost: await page.getByLabel("Subscription host", { exact: true }).inputValue()
  };
}

async function applyQaSettings(page, settings) {
  const changed = await readSettingsFromUI(page);
  let didChange = false;

  if (changed.configPath !== settings.configPath) {
    await fillField(page, "Config path", settings.configPath);
    didChange = true;
  }
  if (settings.grpcAddress && changed.grpcAddress !== settings.grpcAddress) {
    await fillField(page, "gRPC address", settings.grpcAddress);
    didChange = true;
  }
  if (settings.publicBaseUrl && changed.publicBaseUrl !== settings.publicBaseUrl) {
    await fillField(page, "Public base URL", settings.publicBaseUrl);
    didChange = true;
  }
  if (settings.subscriptionHost && changed.subscriptionHost !== settings.subscriptionHost) {
    await fillField(page, "Subscription host", settings.subscriptionHost);
    didChange = true;
  }

  if (didChange) {
    await page.getByRole("button", { name: "Save Settings", exact: true }).click();
    await waitForTopMessage(page, /Settings saved/i);
  }

  return {
    configPath: settings.configPath
  };
}

async function restoreOriginalSettings(page, original) {
  await nav(page, "Settings");
  await fillField(page, "Config path", original.configPath);
  await fillField(page, "gRPC address", original.grpcAddress);
  await fillField(page, "Public base URL", original.publicBaseUrl);
  await fillField(page, "Subscription host", original.subscriptionHost);
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await waitForTopMessage(page, /Settings saved/i);
}

async function isTransportAvailable(page, transport) {
  await selectTab(page, "Transport");
  const selector = page.getByLabel("Network", { exact: true });
  const options = await selector.locator("option").evaluateAll((items) => items.map((item) => item.value));
  return options.includes(transport);
}

async function openNewInbound(page) {
  await closeDrawer(page);
  await nav(page, "Inbounds");
  await page.getByRole("button", { name: "New Inbound", exact: true }).click();
}

async function openExistingInbound(page, tag) {
  await closeDrawer(page);
  await nav(page, "Inbounds");
  await page.getByRole("button", { name: tag, exact: true }).click();
  await page.getByRole("button", { name: "Save Inbound", exact: true }).waitFor();
}

async function selectTab(page, tabName) {
  await page.getByRole("button", { name: tabName, exact: true }).click();
  await page.waitForTimeout(80);
}

async function selectField(page, label, value) {
  await page.getByLabel(label, { exact: true }).selectOption(value);
}

async function fillField(page, label, value) {
  await page.getByLabel(label, { exact: true }).click({ delay: 20 });
  await page.getByLabel(label, { exact: true }).fill(String(value));
}

async function fillProtocolFields(page, protocol, values) {
  if (protocol === "vless" || protocol === "vmess") {
    const field = protocol === "vless" ? "Decryption" : "Encryption";
    const payload = protocol === "vless" ? values.decryption || "none" : values.encryption || "auto";
    await fillField(page, field, payload);
  }
  if (protocol === "shadowsocks") {
    await fillField(page, "Method", values.method || "2022-blake3-aes-128-gcm");
  }
}

async function fillTransportFields(page, network, values) {
  if (network === "ws") {
    await fillField(page, "Path", values.path || "/");
    await fillField(page, "Host header", values.host || "");
  }
  if (network === "grpc") {
    await fillField(page, "Service name", values.serviceName || "GunService");
  }
  if (network === "httpupgrade") {
    await fillField(page, "Path", values.path || "/");
    await fillField(page, "Host", values.host || "");
  }
  if (network === "splithttp") {
    await fillField(page, "Path", values.path || "/");
  }
}

async function fillSecurityFields(page, security, values) {
  if (security === "tls") {
    await fillField(page, "Server name", values.serverName || "example.com");
    await fillField(page, "ALPN", values.alpn || "h2,http/1.1");
    await fillField(page, "Certificate file", values.certificateFile || "/etc/blackwire/fullchain.pem");
    await fillField(page, "Key file", values.keyFile || "/etc/blackwire/privkey.pem");
  }
  if (security === "reality") {
    await fillField(page, "Server name", values.serverName || "www.cloudflare.com");
    await fillField(page, "Public key", values.publicKey || "r3Yc3...");
    await fillField(page, "Short ID", values.shortId || "6ba85179e30d4fc2");
    await fillField(page, "Fingerprint", values.fingerprint || "chrome");
    await fillField(page, "Spider X", values.spiderX || "/");
  }
}

async function fillSniffing(page, sniffing, limits) {
  if (sniffing.enabled) {
    const sn = page.getByRole("switch", { name: "Sniffing enabled", exact: true });
    if ((await sn.getAttribute("aria-checked")) !== "true") await sn.click();
  }
  for (const item of sniffing.destOverride || []) {
    const chip = page.locator(".toggle-chip", { hasText: item });
    if (await chip.count()) {
      await chip.first().click();
    }
  }
  if (sniffing.metadataOnly) {
    const sw = page.getByRole("switch", { name: "Metadata only", exact: true });
    if ((await sw.getAttribute("aria-checked")) !== "true") await sw.click();
  }
  if (sniffing.routeOnly) {
    const sw = page.getByRole("switch", { name: "Route only", exact: true });
    if ((await sw.getAttribute("aria-checked")) !== "true") await sw.click();
  }
  if (limits.maxConnections) {
    await fillField(page, "Max connections", limits.maxConnections);
  }
  if (limits.maxHandshakeSeconds) {
    await fillField(page, "Max handshake seconds", limits.maxHandshakeSeconds);
  }
}

async function fillAdvancedJSON(page, settingsValue, streamValue) {
  const editors = page.locator(".advanced-slice textarea");
  if (settingsValue) {
    await editors.nth(0).fill(JSON.stringify(settingsValue, null, 2));
  }
  if (streamValue) {
    await editors.nth(1).fill(JSON.stringify(streamValue, null, 2));
  }
}

async function nav(page, name) {
  await page.getByRole("button", { name, exact: true }).click();
}

async function waitForInboundTag(page, tag) {
  await page.getByRole("button", { name: tag, exact: true }).waitFor({ timeout: 12000 });
}

async function countInboundRows(page) {
  await nav(page, "Inbounds");
  return page.locator("tbody tr").count();
}

async function closeDrawer(page) {
  const close = page.getByLabel("Close", { exact: true });
  if (await close.isVisible().catch(() => false)) await close.click();
}

async function waitForTopMessage(page, pattern, timeout = 12000) {
  await page.waitForFunction(
    (source) => {
      const msg = document.querySelector(".strip-message")?.textContent ?? "";
      return new RegExp(source, "i").test(msg);
    },
    pattern.source,
    { timeout }
  );
}

async function waitForConfigSaved(page, timeout = 15000) {
  await page.waitForFunction(
    () => {
      const msg = document.querySelector(".strip-message")?.textContent ?? "";
      return /config saved|live runtime synchronized|gRPC unavailable|live apply failed|live gRPC disabled/i.test(msg);
    },
    { timeout }
  );
}

async function getTopMessageText(page) {
  return (await page.locator(".strip-message").textContent()) ?? "";
}

async function hasText(page, needle) {
  try {
    return await page.getByText(new RegExp(escapeRegExp(needle), "i")).isVisible();
  } catch {
    return false;
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function launchBrowser() {
  const attempts = [
    { headless: !headed },
    { headless: !headed, channel: "chrome" },
    { headless: !headed, channel: "msedge" }
  ];
  for (const attempt of attempts) {
    try {
      return await chromium.launch(attempt);
    } catch {
      // try next installed binary
    }
  }
  return null;
}

async function readInboundFromDisk(configPath, tag) {
  const raw = await readFile(configPath, "utf8");
  const config = JSON.parse(raw);
  const inbound = (config.inbounds || []).find((item) => item.tag === tag);
  if (!inbound) return null;
  return {
    ...inbound,
    enabled: inbound.enabled !== false,
    transport: inbound.transport ?? inbound.streamSettings?.network ?? ""
  };
}

async function isInboundInConfig(configPath, tag) {
  const raw = await readFile(configPath, "utf8");
  const config = JSON.parse(raw);
  return (config.inbounds || []).some((item) => item.tag === tag);
}

function assertSubset(actual, expected, path = "root") {
  if (expected === undefined) return;
  if (expected === null) {
    if (actual !== null) throw new Error(`${path}: expected null`);
    return;
  }
  if (actual === undefined || actual === null) {
    throw new Error(`${path}: missing actual value`);
  }
  if (typeof expected !== "object" || expected === null) {
    if (actual !== expected) {
      throw new Error(`${path}: expected ${JSON.stringify(expected)} got ${JSON.stringify(actual)}`);
    }
    return;
  }
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) {
      throw new Error(`${path}: expected array`);
    }
    if (actual.length < expected.length) {
      throw new Error(`${path}: expected array length >= ${expected.length}`);
    }
    expected.forEach((value, index) => assertSubset(actual[index], value, `${path}[${index}]`));
    return;
  }
  for (const [key, value] of Object.entries(expected)) {
    if (!(key in actual)) throw new Error(`${path}: missing key ${key}`);
    assertSubset(actual[key], value, `${path}.${key}`);
  }
}

function printSummary() {
  const statusOrder = ["PASS", "SKIPPED", "FAILED"];
  console.log(`\n=== QA Plan Summary (${runConfigPath}) ===`);
  const grouped = results.sort((a, b) => statusOrder.indexOf(b.status) - statusOrder.indexOf(a.status));
  for (const item of grouped) {
    console.log(`${item.status.padEnd(8)} ${item.name} - ${item.details}`);
  }
  if (skippedCases.length > 0) {
    console.log("\nSkipped cases:");
    for (const item of skippedCases) {
      console.log(`- ${item.name}: ${item.reason}`);
    }
  }
}

function getArg(argvArray, name, fallback) {
  const index = argvArray.indexOf(name);
  if (index === -1 || index === argvArray.length - 1) return fallback;
  const value = argvArray[index + 1];
  return value.startsWith("--") ? fallback : value;
}
