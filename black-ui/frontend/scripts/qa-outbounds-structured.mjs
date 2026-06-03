import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { chromium } from "playwright";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");

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
const defaultConfigPath = path.join(tmpdir(), "black-ui-qa-outbounds", `${qaRunId}-config.json`);
const runConfigPath = configPathArg || defaultConfigPath;
const reportJsonPath = path.join(path.dirname(runConfigPath), `qa-results-${qaRunId}.json`);
const reportMarkdownPath = path.join(repoRoot, "docs", "outbounds-panel-qa.md");

const basePort = 29700;
const cases = [
  {
    id: "freedom",
    name: "Freedom/default",
    protocol: "freedom",
    network: "tcp",
    security: "none",
    port: basePort + 1,
    expected: {
      protocol: "freedom",
      settings: {}
    }
  },
  {
    id: "vless-tcp",
    name: "VLESS/TCP",
    protocol: "vless",
    network: "tcp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 2,
    userId: "459dc0c8-d891-4768-9234-faf11fd26b5d",
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 2, users: [{ id: "459dc0c8-d891-4768-9234-faf11fd26b5d" }] },
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "vless-ws",
    name: "VLESS/WS",
    protocol: "vless",
    network: "ws",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 3,
    userId: "9d0e2d8e-f0a8-4f1d-a9fa-2db8d01ad881",
    transport: { path: "/vless-ws", host: "ws.example.com" },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 3, users: [{ id: "9d0e2d8e-f0a8-4f1d-a9fa-2db8d01ad881" }] },
      streamSettings: { network: "ws", security: "none", wsSettings: { path: "/vless-ws", headers: { Host: "ws.example.com" } } }
    }
  },
  {
    id: "vless-grpc",
    name: "VLESS/gRPC",
    protocol: "vless",
    network: "grpc",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 4,
    userId: "cbec0dd2-66be-4b02-baf2-f2bc6f6de9a3",
    transport: { serviceName: "blackwire-grpc" },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 4, users: [{ id: "cbec0dd2-66be-4b02-baf2-f2bc6f6de9a3" }] },
      streamSettings: { network: "grpc", security: "none", grpcSettings: { serviceName: "blackwire-grpc" } }
    }
  },
  {
    id: "vless-httpupgrade",
    name: "VLESS/HTTPUpgrade",
    protocol: "vless",
    network: "httpupgrade",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 5,
    userId: "84ee1c1f-4a7e-4b78-a66f-5c9372e97aa8",
    transport: { path: "/upgrade", host: "upgrade.example.com" },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 5, users: [{ id: "84ee1c1f-4a7e-4b78-a66f-5c9372e97aa8" }] },
      streamSettings: { network: "httpupgrade", security: "none", httpupgradeSettings: { path: "/upgrade", host: "upgrade.example.com" } }
    }
  },
  {
    id: "vless-splithttp",
    name: "VLESS/SplitHTTP",
    protocol: "vless",
    network: "splithttp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 6,
    userId: "d7e2f5fa-89b1-4bf1-b0d0-9ac40f1f4f59",
    transport: { path: "/packet" },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 6, users: [{ id: "d7e2f5fa-89b1-4bf1-b0d0-9ac40f1f4f59" }] },
      streamSettings: { network: "splithttp", security: "none", splithttpSettings: { path: "/packet" } }
    }
  },
  {
    id: "vless-kcp",
    name: "VLESS/KCP",
    protocol: "vless",
    network: "kcp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 7,
    userId: "1f88f8b5-1e1d-4d4c-94d3-7393f8f0a1d1",
    optional: true,
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 7, users: [{ id: "1f88f8b5-1e1d-4d4c-94d3-7393f8f0a1d1" }] },
      streamSettings: { network: "kcp", security: "none" }
    }
  },
  {
    id: "vless-quic",
    name: "VLESS/QUIC",
    protocol: "vless",
    network: "quic",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 8,
    userId: "c76fb8ee-d8a9-4fb1-9f4d-6e0ff51f4f2e",
    optional: true,
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 8, users: [{ id: "c76fb8ee-d8a9-4fb1-9f4d-6e0ff51f4f2e" }] },
      streamSettings: { network: "quic", security: "none" }
    }
  },
  {
    id: "vless-tls",
    name: "VLESS/TCP+TLS",
    protocol: "vless",
    network: "tcp",
    security: "tls",
    address: "127.0.0.1",
    port: basePort + 9,
    userId: "c8e0f3f9-b0ff-41f4-8e8d-3a5d5d3b7fd2",
    securityValues: {
      serverName: "example.com",
      alpn: "h2,http/1.1",
      certificateFile: "/etc/blackwire/fullchain.pem",
      keyFile: "/etc/blackwire/privkey.pem"
    },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 9, users: [{ id: "c8e0f3f9-b0ff-41f4-8e8d-3a5d5d3b7fd2" }] },
      streamSettings: {
        network: "tcp",
        security: "tls",
        tlsSettings: {
          serverName: "example.com",
          alpn: ["h2", "http/1.1"],
          certificateFile: "/etc/blackwire/fullchain.pem",
          keyFile: "/etc/blackwire/privkey.pem"
        }
      }
    }
  },
  {
    id: "vless-reality",
    name: "VLESS/TCP+REALITY",
    protocol: "vless",
    network: "tcp",
    security: "reality",
    address: "127.0.0.1",
    port: basePort + 10,
    userId: "4c9b0af7-2f39-4b15-b1a6-4d8d5bf0fb82",
    securityValues: {
      serverName: "www.cloudflare.com",
      publicKey: "base64-x25519-public-key",
      shortId: "6ba85179e30d4fc2",
      fingerprint: "chrome",
      spiderX: "/"
    },
    expected: {
      protocol: "vless",
      settings: { address: "127.0.0.1", port: basePort + 10, users: [{ id: "4c9b0af7-2f39-4b15-b1a6-4d8d5bf0fb82" }] },
      streamSettings: {
        network: "tcp",
        security: "reality",
        realitySettings: {
          serverName: "www.cloudflare.com",
          publicKey: "base64-x25519-public-key",
          shortId: "6ba85179e30d4fc2",
          shortIds: ["6ba85179e30d4fc2"],
          fingerprint: "chrome",
          spiderX: "/"
        }
      }
    }
  },
  {
    id: "vmess-tcp",
    name: "VMess/TCP",
    protocol: "vmess",
    network: "tcp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 11,
    userId: "8f1edb46-6bb1-447f-a5de-2d86bb8822cc",
    expected: {
      protocol: "vmess",
      settings: { address: "127.0.0.1", port: basePort + 11, users: [{ id: "8f1edb46-6bb1-447f-a5de-2d86bb8822cc" }] },
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "trojan-tcp",
    name: "Trojan/TCP",
    protocol: "trojan",
    network: "tcp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 12,
    password: "qa-trojan-password",
    expected: {
      protocol: "trojan",
      settings: { address: "127.0.0.1", port: basePort + 12, password: "qa-trojan-password" },
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "shadowsocks-tcp",
    name: "Shadowsocks/TCP",
    protocol: "shadowsocks",
    network: "tcp",
    security: "none",
    address: "127.0.0.1",
    port: basePort + 13,
    password: "qa-ss-password",
    method: "2022-blake3-aes-128-gcm",
    expected: {
      protocol: "shadowsocks",
      settings: { address: "127.0.0.1", port: basePort + 13, password: "qa-ss-password", method: "2022-blake3-aes-128-gcm" },
      streamSettings: { network: "tcp", security: "none" }
    }
  },
  {
    id: "hysteria2-tcp",
    name: "Hysteria2/TCP",
    protocol: "hysteria2",
    network: "tcp",
    security: "none",
    server: "127.0.0.1:8443",
    expected: {
      protocol: "hysteria2",
      settings: { server: "127.0.0.1:8443" },
      streamSettings: { network: "tcp", security: "none" }
    }
  }
];

const results = [];
const skippedCases = [];
const routingChecks = [];
let originalSettings = null;
let failure = null;

await main();

async function main() {
  await mkdir(path.dirname(runConfigPath), { recursive: true });
  const browser = await launchBrowser();
  if (!browser) {
    throw new Error("No Playwright browser available. Install Chromium or run with a system browser channel.");
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

    originalSettings = await readSettingsFromUI(page);
    await applyQaSettings(page, {
      configPath: runConfigPath,
      grpcAddress: grpcAddressArg || originalSettings.grpcAddress,
      publicBaseUrl: publicBaseUrlArg || originalSettings.publicBaseUrl,
      subscriptionHost: subscriptionHostArg || originalSettings.subscriptionHost,
      adaptiveRoutingEnabled: true
    });

    const createdTags = [];
    for (const testCase of cases) {
      const tag = `${qaRunId}-${testCase.id}`;
      try {
        await runMatrixCase(page, { ...testCase, tag }, runConfigPath);
        if (!testCase.expectFailure && !(testCase.optional && results.at(-1)?.status === "SKIPPED")) {
          createdTags.push(tag);
        }
      } catch (error) {
        results.push({ name: testCase.name, status: "FAILED", details: String(error) });
      }
    }

    await runAdvancedPreserveCase(page, `${qaRunId}-advanced-preserve`, runConfigPath);
    await runValidationGuards(page);
    await runRoutingWorkflow(page, runConfigPath);
    await runDeleteFlow(page, runConfigPath);
    await runEnabledToggleOmission(page, runConfigPath);
    await runNoEnabledFallback(page, runConfigPath);

    await cleanupQaOutbounds(page, createdTags.concat([
      `${qaRunId}-advanced-preserve`,
      `${qaRunId}-delete`,
      `${qaRunId}-toggle`
    ]));

    if (restoreSettings && !keepSettings && originalSettings) {
      await restoreOriginalSettings(page, originalSettings);
    }
  } catch (error) {
    failure = error;
  } finally {
    const report = buildReport();
    await writeFile(reportJsonPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
    await writeMarkdownReport(report);
    await context.close().catch(() => {});
    await browser.close().catch(() => {});
    const relevantConsole = consoleMessages.filter((item) => !item.includes("401"));
    if (relevantConsole.length > 0) {
      console.warn("Browser console noise:");
      relevantConsole.forEach((line) => console.warn(`  ${line}`));
    }
  }

  if (failure) {
    throw failure;
  }
  const failed = results.filter((item) => item.status === "FAILED");
  if (failed.length > 0) {
    process.exitCode = 1;
  }
}

async function runMatrixCase(page, testCase, configPath) {
  if (testCase.optional) {
    try {
      if (!(await isTransportAvailable(page, testCase.network))) {
        results.push({
          name: testCase.name,
          status: "SKIPPED",
          details: `${testCase.network} transport not exposed by the current UI`
        });
        skippedCases.push({ name: testCase.name, reason: `${testCase.network} transport not exposed by the current UI` });
        return;
      }
    } catch {
      results.push({
        name: testCase.name,
        status: "SKIPPED",
        details: `${testCase.network} transport selector could not be reached cleanly`
      });
      skippedCases.push({ name: testCase.name, reason: `${testCase.network} transport selector could not be reached cleanly` });
      return;
    }
  }

  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", testCase.tag);
  await selectField(page, "Protocol", testCase.protocol);

  await selectTab(page, "Protocol");
  await fillProtocolFields(page, testCase);

  await selectTab(page, "Transport");
  await selectField(page, "Network", testCase.network);
  await fillTransportFields(page, testCase);

  await selectTab(page, "Security");
  await selectField(page, "Security layer", testCase.security);
  await fillSecurityFields(page, testCase);

  if (testCase.advanced) {
    await selectTab(page, "Advanced");
    await fillAdvancedSlices(page, testCase.advanced.settings, testCase.advanced.streamSettings);
  }

  if (testCase.expectFailure) {
    const saveDisabled = await page.getByRole("button", { name: "Save Outbound", exact: true }).isDisabled();
    const hasHint = await hasText(page, testCase.expectedFailureHint ?? "validation");
    if (!saveDisabled && !hasHint) {
      await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
      let saved = false;
      try {
        await waitForSaveMessage(page, /config saved|Outbound saved/i);
        saved = true;
      } catch {
        // expected path: backend/UI rejected the payload
      }
      if (saved) {
        throw new Error("expected failure, but outbound saved");
      }
    }
    if (!saveDisabled && !hasHint) {
      throw new Error(`expected failure hint not found: ${testCase.expectedFailureHint ?? "validation"}`);
    }
    results.push({
      name: testCase.name,
      status: "PASS",
      details: `validation guard confirmed (${testCase.expectedFailureHint ?? "blocked"})`
    });
    return;
  }

  const saveButton = page.getByRole("button", { name: "Save Outbound", exact: true });
  if (await saveButton.isDisabled()) {
    const saveErrors = await currentDrawerErrors(page);
    await closeDrawer(page);
    throw new Error(`${testCase.name}: Save Outbound disabled (${saveErrors.join("; ") || "unknown validation issue"})`);
  }

  await saveButton.click();
  await waitForSaveMessage(page);

  await nav(page, "Outbounds");
  await waitForOutboundTag(page, testCase.tag);
  const outbound = await readOutboundFromDisk(configPath, testCase.tag);
  if (!outbound) {
    throw new Error(`expected ${testCase.tag} to exist in ${configPath}`);
  }
  assertSubset(outbound, testCase.expected, testCase.tag);
  results.push({
    name: testCase.name,
    status: "PASS",
    details: `${testCase.tag} persisted to ${configPath}`
  });
}

async function runAdvancedPreserveCase(page, tag, configPath) {
  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", tag);
  await selectField(page, "Protocol", "vless");
  await selectTab(page, "Protocol");
  await fillProtocolFields(page, {
    protocol: "vless",
    address: "127.0.0.1",
    port: basePort + 40,
    userId: "b6d9a819-7d1c-4a68-8dd5-2d64c8f0dd91"
  });
  await selectTab(page, "Transport");
  await selectField(page, "Network", "ws");
  await fillTransportFields(page, {
    network: "ws",
    transport: { path: "/adv", host: "adv.example.com" }
  });
  await selectTab(page, "Advanced");
  await fillAdvancedSlices(
    page,
    {
      address: "127.0.0.1",
      port: basePort + 40,
      users: [{ id: "b6d9a819-7d1c-4a68-8dd5-2d64c8f0dd91" }],
      _qaKeepSettings: { marker: "keep-me", nested: { ok: true } }
    },
    {
      network: "ws",
      security: "none",
      wsSettings: { path: "/adv", headers: { Host: "adv.example.com" } },
      _qaKeepTransport: { marker: "preserve", nested: { value: 1 } }
    }
  );

  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);

  await openOutbound(page, tag);
  await selectTab(page, "Transport");
  await fillField(page, "Host header", "changed.example.com");
  await selectTab(page, "Advanced");
  const settingsEditor = page.locator(".advanced-slice textarea").nth(0);
  const streamEditor = page.locator(".advanced-slice textarea").nth(1);
  await settingsEditor.evaluate((el) => el.scrollIntoView({ block: "center" }));
  await streamEditor.evaluate((el) => el.scrollIntoView({ block: "center" }));
  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);

  const outbound = await readOutboundFromDisk(configPath, tag);
  assertSubset(
    outbound,
    {
      protocol: "vless",
      settings: { _qaKeepSettings: { marker: "keep-me", nested: { ok: true } } },
      streamSettings: {
        wsSettings: { headers: { Host: "changed.example.com" } },
        _qaKeepTransport: { marker: "preserve", nested: { value: 1 } }
      }
    },
    tag
  );
  results.push({
    name: "Advanced JSON preserve",
    status: "PASS",
    details: `${tag} preserved unknown keys while structured fields changed`
  });
}

async function runValidationGuards(page) {
  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", `${qaRunId}-bad-uuid`);
  await selectField(page, "Protocol", "vless");
  await selectTab(page, "Protocol");
  await fillField(page, "Address", "127.0.0.1");
  await fillField(page, "Port", String(basePort + 50));
  await fillField(page, "User ID", "not-a-uuid");
  const invalidUuidDisabled = await page.getByRole("button", { name: "Save Outbound", exact: true }).isDisabled();
  const invalidUuidHint = await hasText(page, "valid UUID");
  results.push({
    name: "Invalid UUID guard",
    status: invalidUuidDisabled && invalidUuidHint ? "PASS" : "FAILED",
    details: invalidUuidDisabled && invalidUuidHint ? "invalid UUID blocked before save" : "invalid UUID was not blocked"
  });
  await closeDrawer(page);

  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", `${qaRunId}-bad-json`);
  await selectField(page, "Protocol", "trojan");
  await selectTab(page, "Protocol");
  await fillField(page, "Address", "127.0.0.1");
  await fillField(page, "Port", String(basePort + 51));
  await fillField(page, "Password", "qa-password");
  await selectTab(page, "Advanced");
  const editor = page.locator(".advanced-slice textarea").nth(0);
  await editor.fill("{");
  const jsonDisabled = await page.getByRole("button", { name: "Save Outbound", exact: true }).isDisabled();
  const jsonHint = await hasText(page, "Invalid JSON");
  results.push({
    name: "Invalid JSON guard",
    status: jsonDisabled && jsonHint ? "PASS" : "FAILED",
    details: jsonDisabled && jsonHint ? "malformed advanced JSON blocked before save" : "invalid JSON was not blocked"
  });
  await closeDrawer(page);

  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", `${qaRunId}-bad-password`);
  await selectField(page, "Protocol", "trojan");
  await selectTab(page, "Protocol");
  await fillField(page, "Address", "127.0.0.1");
  await fillField(page, "Port", String(basePort + 52));
  const passwordHint = await hasText(page, "Trojan outbound requires a password.");
  const passwordDisabled = await page.getByRole("button", { name: "Save Outbound", exact: true }).isDisabled();
  results.push({
    name: "Missing password guard",
    status: passwordDisabled && passwordHint ? "PASS" : "FAILED",
    details: passwordDisabled && passwordHint ? "trojan password guard shown" : "trojan password guard missing"
  });
  await closeDrawer(page);
}

async function runRoutingWorkflow(page, configPath) {
  await nav(page, "Settings");
  await ensureSwitch(page, "Auto adaptive routing for enabled outbounds", true);
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await waitForTopMessage(page, /Settings saved/i);

  await nav(page, "Advanced Config");
  await page.getByRole("button", { name: /routing/i }).first().click();
  await page.waitForTimeout(120);
  const adaptiveTemplate = page.getByRole("button", { name: "Adaptive Template", exact: true });
  if (!(await adaptiveTemplate.isEnabled())) {
    throw new Error("Adaptive Template button was not enabled even though adaptive routing and enabled outbounds were present");
  }
  await adaptiveTemplate.click();
  await page.getByRole("button", { name: "Save Advanced Config", exact: true }).click();
  await waitForSaveMessage(page);

  const config = await readConfig(configPath);
  const enabledTags = (config.outbounds || []).map((item) => item.tag);
  const routing = config.routing ?? {};
  const balancer = routing.balancers?.[0];
  if (!balancer) {
    throw new Error("adaptive routing did not generate a balancer");
  }
  assertSubset(
    routing,
    {
      balancers: [
        {
          tag: "auto-proxy",
          selector: enabledTags,
          strategy: "adaptive",
          profiles: enabledTags.map((tag, index) => ({
            name: index === 0 ? "stable" : `backup-${index}`,
            outboundTag: tag
          }))
        }
      ],
      rules: [{ outboundTag: "auto-proxy" }]
    },
    "routing"
  );
  routingChecks.push({
    name: "Adaptive routing workflow",
    status: "PASS",
    details: `routing balancer references ${enabledTags.length} enabled outbounds`
  });
}

async function runDeleteFlow(page, configPath) {
  const tag = `${qaRunId}-delete`;
  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", tag);
  await selectField(page, "Protocol", "freedom");
  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);

  await openOutbound(page, tag);
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await waitForSaveMessage(page);
  await nav(page, "Outbounds");
  await page.waitForTimeout(250);
  const config = await readConfig(configPath);
  const deleted = !(config.outbounds || []).some((item) => item.tag === tag);
  results.push({
    name: "Delete flow",
    status: deleted ? "PASS" : "FAILED",
    details: deleted ? `${tag} removed from config output` : `${tag} still present after delete`
  });
}

async function runEnabledToggleOmission(page, configPath) {
  const tag = `${qaRunId}-toggle`;
  await openNewOutbound(page);
  await selectTab(page, "Basic");
  await fillField(page, "Tag", tag);
  await selectField(page, "Protocol", "vless");
  await selectTab(page, "Protocol");
  await fillField(page, "Address", "127.0.0.1");
  await fillField(page, "Port", String(basePort + 60));
  await fillField(page, "User ID", "a5d2d87d-5c39-44cb-9c48-9b5a1e8cfd6f");
  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);

  await openOutbound(page, tag);
  await setDrawerEnabled(page, false);
  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);
  const config = await readConfig(configPath);
  const omitted = !(config.outbounds || []).some((item) => item.tag === tag);
  results.push({
    name: "Disabled outbound omission",
    status: omitted ? "PASS" : "FAILED",
    details: omitted ? `${tag} omitted from generated config when disabled` : `${tag} remained in generated config`
  });
  await openOutbound(page, tag);
  await setDrawerEnabled(page, true);
  await page.getByRole("button", { name: "Save Outbound", exact: true }).click();
  await waitForSaveMessage(page);
}

async function runNoEnabledFallback(page, configPath) {
  const outbounds = await listOutbounds(page);
  const enabled = outbounds.filter((item) => item.enabled).map((item) => item.tag);
  const toggled = [];
  for (const outbound of outbounds.filter((item) => item.enabled)) {
    await setOutboundEnabled(page, outbound, false);
    toggled.push(outbound);
  }

  const config = await readConfig(configPath);
  const fallback = config.outbounds || [];
  const passed = fallback.length === 1 && fallback[0].tag === "freedom" && fallback[0].protocol === "freedom";
  results.push({
    name: "No-enabled fallback",
    status: passed ? "PASS" : "FAILED",
    details: passed ? "generated config fell back to synthetic freedom outbound" : `unexpected outbounds array: ${JSON.stringify(fallback)}`
  });

  for (const outbound of toggled) {
    await setOutboundEnabled(page, outbound, true);
  }
}

async function cleanupQaOutbounds(page, tags) {
  const outbounds = await listOutbounds(page);
  for (const outbound of outbounds.filter((item) => tags.includes(item.tag))) {
    await deleteOutbound(page, outbound.id);
  }
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
    () => Boolean(document.querySelector("aside.sidebar")) || Array.from(document.querySelectorAll("h1")).some((node) => /Users|Inbounds|Outbounds|Advanced Config|Settings/i.test(node.textContent ?? "")),
    { timeout: 12000 }
  );
}

async function primeAuthCookie(context) {
  try {
    const db = new DatabaseSync(path.join(repoRoot, "black-ui", "data", "black-ui.db"));
    const row = db.prepare("SELECT token FROM sessions ORDER BY created_at DESC LIMIT 1").get();
    if (!row?.token) return;
    await context.addCookies([
      {
        name: "black_ui_session",
        value: row.token,
        url: uiUrl
      }
    ]);
  } catch {
    // Fallback to the normal auth flow.
  }
}

async function readSettingsFromUI(page) {
  await nav(page, "Settings");
  return {
    configPath: await page.getByLabel("Config path", { exact: true }).inputValue(),
    grpcAddress: await page.getByLabel("gRPC address", { exact: true }).inputValue(),
    publicBaseUrl: await page.getByLabel("Public base URL", { exact: true }).inputValue(),
    subscriptionHost: await page.getByLabel("Subscription host", { exact: true }).inputValue(),
    adaptiveRoutingEnabled: await page
      .getByRole("switch", { name: "Auto adaptive routing for enabled outbounds", exact: true })
      .getAttribute("aria-checked")
      .then((value) => value === "true")
  };
}

async function applyQaSettings(page, settings) {
  const current = await readSettingsFromUI(page);
  let didChange = false;

  if (current.configPath !== settings.configPath) {
    await fillField(page, "Config path", settings.configPath);
    didChange = true;
  }
  if (settings.grpcAddress && current.grpcAddress !== settings.grpcAddress) {
    await fillField(page, "gRPC address", settings.grpcAddress);
    didChange = true;
  }
  if (settings.publicBaseUrl && current.publicBaseUrl !== settings.publicBaseUrl) {
    await fillField(page, "Public base URL", settings.publicBaseUrl);
    didChange = true;
  }
  if (settings.subscriptionHost && current.subscriptionHost !== settings.subscriptionHost) {
    await fillField(page, "Subscription host", settings.subscriptionHost);
    didChange = true;
  }
  if (settings.adaptiveRoutingEnabled !== undefined && current.adaptiveRoutingEnabled !== settings.adaptiveRoutingEnabled) {
    await ensureSwitch(page, "Auto adaptive routing for enabled outbounds", settings.adaptiveRoutingEnabled);
    didChange = true;
  }

  if (didChange) {
    await page.getByRole("button", { name: "Save Settings", exact: true }).click();
    await waitForTopMessage(page, /Settings saved/i);
  }
}

async function restoreOriginalSettings(page, settings) {
  await nav(page, "Settings");
  await fillField(page, "Config path", settings.configPath);
  await fillField(page, "gRPC address", settings.grpcAddress);
  await fillField(page, "Public base URL", settings.publicBaseUrl);
  await fillField(page, "Subscription host", settings.subscriptionHost);
  await ensureSwitch(page, "Auto adaptive routing for enabled outbounds", settings.adaptiveRoutingEnabled);
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await waitForTopMessage(page, /Settings saved/i);
}

async function openNewOutbound(page) {
  await closeDrawer(page);
  await nav(page, "Outbounds");
  await page.getByRole("button", { name: "New Outbound", exact: true }).click();
}

async function nav(page, name) {
  await page.getByRole("button", { name, exact: true }).click();
}

async function openOutbound(page, tag) {
  await closeDrawer(page);
  await nav(page, "Outbounds");
  await page.getByRole("button", { name: tag, exact: true }).click();
  await page.getByRole("button", { name: "Save Outbound", exact: true }).waitFor();
}

async function selectTab(page, tabName) {
  await page.getByRole("button", { name: tabName, exact: true }).click();
  await page.waitForTimeout(80);
}

async function selectField(page, label, value) {
  await page.getByLabel(label, { exact: true }).selectOption(value);
}

async function fillField(page, label, value) {
  const field = page.getByLabel(label, { exact: true });
  await field.click({ delay: 20 });
  await field.fill(String(value));
}

async function fillProtocolFields(page, testCase) {
  if (testCase.protocol === "vless" || testCase.protocol === "vmess") {
    await fillField(page, "Address", testCase.address);
    await fillField(page, "Port", String(testCase.port));
    await fillField(page, "User ID", testCase.userId);
  }
  if (testCase.protocol === "trojan" || testCase.protocol === "shadowsocks") {
    await fillField(page, "Address", testCase.address);
    await fillField(page, "Port", String(testCase.port));
    await fillField(page, "Password", testCase.password);
  }
  if (testCase.protocol === "shadowsocks") {
    await fillField(page, "Method", testCase.method);
  }
  if (testCase.protocol === "hysteria2") {
    await fillField(page, "Server", testCase.server);
  }
}

async function fillTransportFields(page, testCase) {
  const transport = testCase.transport || {};
  if (testCase.network === "ws") {
    await fillField(page, "Path", transport.path || "/");
    await fillField(page, "Host header", transport.host || "");
  }
  if (testCase.network === "grpc") {
    await fillField(page, "Service name", transport.serviceName || "GunService");
  }
  if (testCase.network === "httpupgrade") {
    await fillField(page, "Path", transport.path || "/");
    await fillField(page, "Host", transport.host || "");
  }
  if (testCase.network === "splithttp") {
    await fillField(page, "Path", transport.path || "/");
  }
}

async function fillSecurityFields(page, testCase) {
  const security = testCase.securityValues || {};
  if (testCase.security === "tls") {
    await fillField(page, "Server name", security.serverName || "example.com");
    await fillField(page, "ALPN", security.alpn || "h2,http/1.1");
    await fillField(page, "Certificate file", security.certificateFile || "/etc/blackwire/fullchain.pem");
    await fillField(page, "Key file", security.keyFile || "/etc/blackwire/privkey.pem");
  }
  if (testCase.security === "reality") {
    await fillField(page, "Server name", security.serverName || "www.cloudflare.com");
    await fillField(page, "Public key", security.publicKey || "base64-x25519-public-key");
    await fillField(page, "Short ID", security.shortId || "6ba85179e30d4fc2");
    await fillField(page, "Fingerprint", security.fingerprint || "chrome");
    await fillField(page, "Spider X", security.spiderX || "/");
  }
}

async function fillAdvancedSlices(page, settingsValue, streamValue) {
  const editors = page.locator(".advanced-slice textarea");
  if (settingsValue) {
    await editors.nth(0).fill(JSON.stringify(settingsValue, null, 2));
  }
  if (streamValue) {
    await editors.nth(1).fill(JSON.stringify(streamValue, null, 2));
  }
}

async function ensureSwitch(page, label, checked) {
  const sw = page.getByRole("switch", { name: label, exact: true });
  const current = (await sw.getAttribute("aria-checked")) === "true";
  if (current !== checked) {
    await sw.click();
  }
}

async function setDrawerEnabled(page, checked) {
  const sw = page.locator(".drawer [role='switch']").first();
  const current = (await sw.getAttribute("aria-checked")) === "true";
  if (current !== checked) {
    await sw.click();
  }
}

async function hasText(page, needle) {
  try {
    return await page.getByText(new RegExp(escapeRegExp(needle), "i")).isVisible();
  } catch {
    return false;
  }
}

async function closeDrawer(page) {
  const close = page.getByLabel("Close", { exact: true });
  if (await close.isVisible().catch(() => false)) {
    await close.click();
  }
}

async function waitForTopMessage(page, pattern, timeout = 12000) {
  const previous = await page.locator(".strip-message").textContent().catch(() => "");
  try {
    await page.waitForFunction(
      (prev) => (document.querySelector(".strip-message")?.textContent ?? "") !== prev,
      previous ?? "",
      { timeout: Math.min(timeout, 4000) }
    );
  } catch {
    // If the message updates too quickly to catch the intermediate state, continue to the final assertion.
  }
  await page.waitForFunction(
    (source) => {
      const msg = document.querySelector(".strip-message")?.textContent ?? "";
      return new RegExp(source, "i").test(msg);
    },
    pattern.source,
    { timeout }
  );
}

async function waitForSaveMessage(page, timeout = 15000) {
  const previous = await page.locator(".strip-message").textContent().catch(() => "");
  try {
    await page.waitForFunction(
      (prev) => (document.querySelector(".strip-message")?.textContent ?? "") !== prev,
      previous ?? "",
      { timeout: Math.min(timeout, 5000) }
    );
  } catch {
    // Fall through to the final state assertion if the pending state was too brief to observe.
  }
  await page.waitForFunction(
    (source) => {
      const msg = document.querySelector(".strip-message")?.textContent ?? "";
      return new RegExp(source, "i").test(msg);
    },
    "config saved|Outbound saved|live runtime synchronized|gRPC unavailable|live apply failed|live gRPC disabled",
    { timeout }
  );
}

async function waitForOutboundTag(page, tag) {
  await page.getByRole("button", { name: tag, exact: true }).waitFor({ timeout: 12000 });
}

async function isTransportAvailable(page, transport) {
  await selectTab(page, "Transport");
  const selector = page.getByLabel("Network", { exact: true });
  const options = await selector.locator("option").evaluateAll((items) => items.map((item) => item.value));
  return options.includes(transport);
}

async function currentDrawerErrors(page) {
  return page.locator(".drawer .field-error, .drawer .error-line").evaluateAll((items) => items.map((item) => item.textContent?.trim() || "").filter(Boolean)).catch(() => []);
}

async function readConfig(configPath) {
  const raw = await readFile(configPath, "utf8");
  return JSON.parse(raw);
}

async function listOutbounds(page) {
  return page.evaluate(async () => {
    const response = await fetch("/api/outbounds");
    return response.json();
  });
}

async function setOutboundEnabled(page, outbound, enabled) {
  await page.evaluate(
    async ({ id, tag, protocol, settings, streamSettings, enabled: nextEnabled }) => {
      const response = await fetch(`/api/outbounds/${id}`, {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
          "X-Black-UI-Request": "fetch"
        },
        body: JSON.stringify({
          tag,
          protocol,
          enabled: nextEnabled,
          settings,
          streamSettings
        })
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      return response.json();
    },
    {
      id: outbound.id,
      tag: outbound.tag,
      protocol: outbound.protocol,
      settings: outbound.settings,
      streamSettings: outbound.streamSettings,
      enabled
    }
  );
}

async function deleteOutbound(page, id) {
  await page.evaluate(
    async ({ outboundId }) => {
      const response = await fetch(`/api/outbounds/${outboundId}`, {
        method: "DELETE",
        headers: {
          "X-Black-UI-Request": "fetch"
        }
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      return response.json();
    },
    { outboundId: id }
  );
}

async function readOutboundFromDisk(configPath, tag) {
  const config = await readConfig(configPath);
  const outbound = (config.outbounds || []).find((item) => item.tag === tag);
  return outbound ? outbound : null;
}

function assertSubset(actual, expected, pathLabel = "root") {
  if (expected === undefined) return;
  if (expected === null) {
    if (actual !== null) throw new Error(`${pathLabel}: expected null`);
    return;
  }
  if (actual === undefined || actual === null) {
    throw new Error(`${pathLabel}: missing actual value`);
  }
  if (typeof expected !== "object" || expected === null) {
    if (actual !== expected) {
      throw new Error(`${pathLabel}: expected ${JSON.stringify(expected)} got ${JSON.stringify(actual)}`);
    }
    return;
  }
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) {
      throw new Error(`${pathLabel}: expected array`);
    }
    if (actual.length < expected.length) {
      throw new Error(`${pathLabel}: expected array length >= ${expected.length}`);
    }
    expected.forEach((value, index) => assertSubset(actual[index], value, `${pathLabel}[${index}]`));
    return;
  }
  for (const [key, value] of Object.entries(expected)) {
    if (!(key in actual)) throw new Error(`${pathLabel}: missing key ${key}`);
    assertSubset(actual[key], value, `${pathLabel}.${key}`);
  }
}

function buildReport() {
  return {
    generatedAt: new Date().toISOString(),
    panelUrl: uiUrl,
    configPath: runConfigPath,
    counts: {
      passed: results.filter((item) => item.status === "PASS").length,
      skipped: results.filter((item) => item.status === "SKIPPED").length,
      failed: results.filter((item) => item.status === "FAILED").length
    },
    results,
    skippedCases,
    routingChecks,
    failure: failure ? String(failure) : ""
  };
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function writeMarkdownReport(report) {
  const lines = [];
  lines.push("# Outbounds Panel QA Result");
  lines.push("");
  lines.push("## Run Summary");
  lines.push(`- Panel URL: \`${report.panelUrl}\``);
  lines.push(`- Disposable config path: \`${report.configPath}\``);
  lines.push(`- Report JSON: \`${reportJsonPath}\``);
  lines.push(`- Passed: ${report.counts.passed}`);
  lines.push(`- Failed: ${report.counts.failed}`);
  lines.push(`- Skipped: ${report.counts.skipped}`);
  lines.push("");
  lines.push("## Results");
  for (const item of report.results) {
    lines.push(`- ${item.status}: ${item.name} - ${item.details}`);
  }
  if (report.results.length === 0) {
    lines.push("- No matrix cases ran.");
  }
  lines.push("");
  lines.push("## Routing");
  if (report.routingChecks.length > 0) {
    for (const item of report.routingChecks) {
      lines.push(`- ${item.status}: ${item.name} - ${item.details}`);
    }
  } else {
    lines.push("- No routing checks recorded.");
  }
  lines.push("");
  lines.push("## Skipped Cases");
  if (report.skippedCases.length > 0) {
    for (const item of report.skippedCases) {
      lines.push(`- ${item.name}: ${item.reason}`);
    }
  } else {
    lines.push("- None.");
  }
  lines.push("");
  lines.push("## Notes");
  lines.push("- The live panel was exercised through the structured outbound editor and Advanced Config routing workflow.");
  lines.push("- Disk assertions were performed against the generated `config.json` after successful saves.");
  if (report.failure) {
    lines.push(`- Run ended with error: \`${report.failure}\``);
  }
  lines.push("");
  await writeFile(reportMarkdownPath, `${lines.join("\n")}\n`, "utf8");
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

function getArg(argvArray, name, fallback) {
  const index = argvArray.indexOf(name);
  if (index === -1 || index === argvArray.length - 1) return fallback;
  const value = argvArray[index + 1];
  return value.startsWith("--") ? fallback : value;
}
