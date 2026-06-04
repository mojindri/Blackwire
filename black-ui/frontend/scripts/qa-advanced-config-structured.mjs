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
const keepSettings = argv.has("--keep-settings");

const qaRunId = `qa-${Date.now().toString(36)}`;
const defaultConfigPath = path.join(tmpdir(), "black-ui-qa-advanced-config", `${qaRunId}-config.json`);
const runConfigPath = configPathArg || defaultConfigPath;
const reportJsonPath = path.join(path.dirname(runConfigPath), `qa-results-${qaRunId}.json`);
const reportMarkdownPath = path.join(repoRoot, "docs", "advanced-config-panel-qa.md");
const qaOutboundTags = [`${qaRunId}-adv-a`, `${qaRunId}-adv-b`];
let originalRoutingSection = null;

const results = [];
const skippedCases = [];
const restorationNotes = [];
const consoleMessages = [];

let originalSettings = null;
let originalSections = [];
let originalOutbounds = [];
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

  page.on("console", (msg) => {
    if (["error", "warning"].includes(msg.type())) {
      consoleMessages.push(`${msg.type()}: ${msg.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    consoleMessages.push(`pageerror: ${error.message}`);
  });
  page.setDefaultTimeout(12000);

  try {
    await page.goto(uiUrl, { waitUntil: "networkidle" });
    await ensureAuthenticated(page);

    await resetConfigDbToDefaults();
    await refreshPanel(page);

    await nav(page, "Settings");
    originalSettings = await readSettingsFromUI(page);
    await purgeQaOutbounds();
    originalSections = await readSections(page);
    originalRoutingSection = originalSections.find((section) => section.name === "routing") ?? null;
    originalOutbounds = await readOutbounds(page);

    await applySettings(page, {
      ...originalSettings,
      configPath: runConfigPath,
      grpcAddress: grpcAddressArg || originalSettings.grpcAddress,
      publicBaseUrl: publicBaseUrlArg || originalSettings.publicBaseUrl,
      subscriptionHost: subscriptionHostArg || originalSettings.subscriptionHost,
      adaptiveRoutingEnabled: false
    });

    await ensureQaOutbounds(page);
    await refreshPanel(page);
    await disableOriginalEnabledOutbounds(page, originalOutbounds);
    await refreshPanel(page);

    await runCase("API structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "api");
      await ensureSectionSwitch(page, true);
      await editorPanel(page).getByLabel("Listen", { exact: true }).fill(originalSettings.grpcAddress);
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(config.api, { listen: originalSettings.grpcAddress }, "api");
      results.push({
        name: "API structured editor",
        status: "PASS",
        details: `API listener persisted as ${originalSettings.grpcAddress}`
      });
    });

    await runCase("Routing structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "routing");
      await ensureSectionSwitch(page, true);
      await resetRoutingCollections(page);
      const panel = editorPanel(page);
      await panel.getByLabel("Domain strategy", { exact: true }).fill("AsIs");
      await panel.getByLabel("GeoIP file", { exact: true }).fill("geoip.dat");
      await panel.getByLabel("Geosite file", { exact: true }).fill("geosite.dat");
      await panel.getByRole("button", { name: "Add Rule", exact: true }).click();
      const ruleCard = panel.locator(".advanced-config-subcard").first();
      await fillRoutingRule(ruleCard, {
        type: "field",
        outboundTag: "auto-proxy",
        port: "443",
        domain: "geosite:google, example.com",
        ip: "geoip:private",
        inboundTag: "vless-main",
        protocol: "http,tls",
        user: "alice@example.com"
      });
      await panel.getByRole("button", { name: "Add Balancer", exact: true }).click();
      const balancerCard = panel.locator(".advanced-config-subcard").nth(1);
      await fillRoutingBalancer(balancerCard, {
        tag: "auto-proxy",
        selector: `${qaOutboundTags[0]}, ${qaOutboundTags[1]}`,
        strategy: "adaptive",
        adaptiveFailureThreshold: "2",
        adaptiveCooldownSecs: "30",
        adaptiveEwmaAlpha: "0.2",
        adaptiveSwitchMargin: "0.15",
        healthUrl: "http://www.gstatic.com/generate_204",
        healthIntervalSecs: "30",
        healthTimeoutSecs: "5",
        healthMaxFailures: "2",
        profiles: [{ name: "stable", outboundTag: qaOutboundTags[0] }]
      });
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(
        config.routing,
        {
          domainStrategy: "AsIs",
          geoipFile: "geoip.dat",
          geositeFile: "geosite.dat",
          rules: [
            {
              type: "field",
              domain: ["geosite:google", "example.com"],
              ip: ["geoip:private"],
              port: "443",
              inboundTag: ["vless-main"],
              protocol: ["http", "tls"],
              user: ["alice@example.com"],
              outboundTag: "auto-proxy"
            }
          ],
          balancers: [
            {
              tag: "auto-proxy",
              selector: [qaOutboundTags[0], qaOutboundTags[1]],
              strategy: "adaptive",
              profiles: [{ name: "stable", outboundTag: qaOutboundTags[0] }],
              adaptive: {
                failureThreshold: 2,
                cooldownSecs: 30,
                ewmaAlpha: 0.2,
                switchMargin: 0.15
              },
              health_check: {
                url: "http://www.gstatic.com/generate_204",
                interval_secs: 30,
                timeout_secs: 5,
                max_failures: 2
              }
            }
          ]
        },
        "routing"
      );
      results.push({
        name: "Routing structured editor",
        status: "PASS",
        details: "routing rules, balancer, and geo files persisted"
      });
    });

    await runCase("Routing adaptive template", async () => {
      await refreshPanel(page);
      await openSection(page, "routing");
      await ensureSectionSwitch(page, true);
      await editorPanel(page).getByRole("button", { name: "Adaptive Template", exact: true }).click();
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(
        config.routing,
        {
          rules: [{ outboundTag: "auto-proxy" }],
          balancers: [
            {
              tag: "auto-proxy",
              selector: [qaOutboundTags[0], qaOutboundTags[1]],
              strategy: "adaptive"
            }
          ]
        },
        "routing-template"
      );
      results.push({
        name: "Routing adaptive template",
        status: "PASS",
        details: "adaptive template generated a schema-compatible routing config"
      });
    });

    await runCase("DNS structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "dns");
      await ensureSectionSwitch(page, true);
      const panel = editorPanel(page);
      const removeButtons = panel.locator(".advanced-config-subcard button", { hasText: "Remove" });
      while ((await removeButtons.count()) > 0) {
        await removeButtons.first().click();
        await page.waitForTimeout(80);
      }
      await panel.getByRole("button", { name: "Add Server", exact: true }).click();
      await panel.getByLabel("Server value", { exact: true }).fill("1.1.1.1");
      await panel.getByRole("button", { name: "Add Server", exact: true }).click();
      await panel.locator(".advanced-config-subcard").nth(1).getByLabel("Server value", { exact: true }).fill("8.8.8.8");
      await ensureScopedSwitch(panel, "Enable FakeIP", true);
      await panel.getByLabel("FakeIP pool", { exact: true }).fill("198.18.0.0/15");
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(
        config.dns,
        {
          servers: ["1.1.1.1", "8.8.8.8"],
          fake_ip: {
            enabled: true,
            pool: "198.18.0.0/15"
          }
        },
        "dns"
      );
      results.push({
        name: "DNS structured editor",
        status: "PASS",
        details: "dns servers and fake_ip persisted"
      });
    });

    await runCase("TUN structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "tun");
      await ensureSectionSwitch(page, true);
      const panel = editorPanel(page);
      await panel.getByLabel("Name", { exact: true }).fill("blackwire-tun");
      await panel.getByLabel("Address", { exact: true }).fill("198.18.0.1");
      await panel.getByLabel("Netmask", { exact: true }).fill("255.255.0.0");
      await panel.getByLabel("MTU", { exact: true }).fill("1501");
      await panel.getByLabel("Bypass mark", { exact: true }).fill("4660");
      await panel.getByLabel("Redirect port", { exact: true }).fill("7890");
      await panel.getByLabel("DNS port", { exact: true }).fill("5300");
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(
        config.tun,
        {
          name: "blackwire-tun",
          address: "198.18.0.1",
          netmask: "255.255.0.0",
          mtu: 1501,
          bypass_mark: 4660,
          redirect_port: 7890,
          dns_port: 5300
        },
        "tun"
      );
      results.push({
        name: "TUN structured editor",
        status: "PASS",
        details: "TUN runtime fields persisted"
      });
    });

    await runCase("Metrics address structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "metricsAddr");
      await ensureSectionSwitch(page, true);
      await editorPanel(page).getByLabel("Metrics address", { exact: true }).fill("127.0.0.1:19090");
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(config.metricsAddr, "127.0.0.1:19090", "metricsAddr");
      results.push({
        name: "Metrics address structured editor",
        status: "PASS",
        details: "metricsAddr persisted as 127.0.0.1:19090"
      });
    });

    await runCase("Profile structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "profile");
      await ensureSectionSwitch(page, true);
      await editorPanel(page).getByLabel("Profile", { exact: true }).selectOption("fast");
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(config.profile, "fast", "profile");
      results.push({
        name: "Profile structured editor",
        status: "PASS",
        details: "profile persisted as fast"
      });
    });

    await runCase("Fast structured editor", async () => {
      await refreshPanel(page);
      await openSection(page, "fast");
      await ensureSectionSwitch(page, true);
      await ensureScopedSwitch(editorPanel(page), "Strict production", false);
      await editorPanel(page).getByLabel("Pool", { exact: true }).selectOption("adaptive");
      await editorPanel(page).getByLabel("Splice", { exact: true }).selectOption("always");
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(
        config.fast,
        {
          strictProduction: false,
          pool: "adaptive",
          splice: "always"
        },
        "fast"
      );
      results.push({
        name: "Fast structured editor",
        status: "PASS",
        details: "fast profile tuning persisted"
      });
    });

    await runCase("Log raw JSON save", async () => {
      await refreshPanel(page);
      await openSection(page, "log");
      const editor = editorPanel(page).locator(".advanced-slice textarea").first();
      await editor.fill(JSON.stringify({ level: "debug", json: true }, null, 2));
      await saveAdvancedConfig(page);
      const config = await readConfigWhenReady(page, runConfigPath);
      assertSubset(config.log, { level: "debug", json: true }, "log");
      results.push({
        name: "Log raw JSON save",
        status: "PASS",
        details: "log section saved as raw JSON"
      });
    });

    await runRawJsonGuard(page, "limits");
    await runRawJsonGuard(page, "stats");

    await restoreOriginalSections(page, originalSections);
    restorationNotes.push("Config sections restored to the original live values.");

    await restoreOriginalOutbounds(page, originalOutbounds);
    restorationNotes.push("Original outbounds restored to their saved enabled states.");

    await deleteQaOutbounds(page, qaOutboundTags);
    restorationNotes.push(`QA outbounds removed: ${qaOutboundTags.join(", ")}.`);

    if (!keepSettings && originalSettings) {
      await applySettings(page, originalSettings);
      restorationNotes.push("Settings restored to their original values.");
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

async function runCase(name, fn) {
  try {
    await fn();
  } catch (error) {
    results.push({
      name,
      status: "FAILED",
      details: String(error instanceof Error ? error.message : error)
    });
  }
}

async function openSection(page, name) {
  await nav(page, "Advanced Config");
  const label = page.locator("button.stack-row strong").filter({ hasText: new RegExp(`^${escapeRegex(name)}$`, "i") }).first();
  const row = label.locator("xpath=ancestor::button[contains(@class,'stack-row')]").first();
  await row.waitFor({ timeout: 10000 });
  await row.scrollIntoViewIfNeeded();
  await row.click();
  await page.waitForTimeout(120);
  await editorPanel(page).getByRole("heading", { name, exact: true }).waitFor({ timeout: 10000 });
}

async function saveAdvancedConfig(page) {
  const panel = editorPanel(page);
  const button = panel.getByRole("button", { name: "Save Advanced Config", exact: true });
  const enabledDeadline = Date.now() + 5000;
  while (Date.now() < enabledDeadline && (await isLocatorDisabled(button))) {
    await page.waitForTimeout(100);
  }
  if (await isLocatorDisabled(button)) {
    const errors = await panel
      .locator(".field-error, .error-line")
      .allTextContents()
      .catch(() => []);
    throw new Error(`Advanced Config save is disabled${errors.length > 0 ? `: ${errors.join(" | ")}` : ""}`);
  }
  await button.click();
  await page.waitForTimeout(500);
}

async function runRawJsonGuard(page, sectionName) {
  await runCase(`Raw JSON guard (${sectionName})`, async () => {
    await refreshPanel(page);
    await openSection(page, sectionName);
    const editor = page.locator(".advanced-slice textarea").first();
    await editor.fill("{");
    const saveDisabled = await page.getByRole("button", { name: "Save Advanced Config", exact: true }).isDisabled();
    const fieldErrorVisible = await page.locator(".field-error").isVisible().catch(() => false);
    const topErrorVisible = await page.locator(".error-line").isVisible().catch(() => false);
    if (!saveDisabled || (!fieldErrorVisible && !topErrorVisible)) {
      throw new Error(`${sectionName} raw JSON error was not surfaced inline`);
    }
    results.push({
      name: `Raw JSON guard (${sectionName})`,
      status: "PASS",
      details: `${sectionName} rejected malformed JSON before save`
    });
  });
}

async function fillRoutingRule(ruleCard, values) {
  await ruleCard.getByLabel("Type", { exact: true }).fill(values.type);
  await ruleCard.getByLabel("Outbound tag", { exact: true }).fill(values.outboundTag);
  await ruleCard.getByLabel("Port", { exact: true }).fill(values.port);
  await ruleCard.getByLabel("Domain CSV", { exact: true }).fill(values.domain);
  await ruleCard.getByLabel("IP CSV", { exact: true }).fill(values.ip);
  await ruleCard.getByLabel("Inbound tag CSV", { exact: true }).fill(values.inboundTag);
  await ruleCard.getByLabel("Protocol CSV", { exact: true }).fill(values.protocol);
  await ruleCard.getByLabel("User CSV", { exact: true }).fill(values.user);
}

async function fillRoutingBalancer(balancerCard, values) {
  await balancerCard.getByLabel("Tag", { exact: true }).fill(values.tag);
  await balancerCard.getByLabel("Selector CSV", { exact: true }).fill(values.selector);
  await balancerCard.getByLabel("Strategy", { exact: true }).selectOption(values.strategy);
  await balancerCard.getByLabel("Failure threshold", { exact: true }).fill(values.adaptiveFailureThreshold);
  await balancerCard.getByLabel("Cooldown seconds", { exact: true }).fill(values.adaptiveCooldownSecs);
  await balancerCard.getByLabel("EWMA alpha", { exact: true }).fill(values.adaptiveEwmaAlpha);
  await balancerCard.getByLabel("Switch margin", { exact: true }).fill(values.adaptiveSwitchMargin);
  await balancerCard.getByLabel("Health URL", { exact: true }).fill(values.healthUrl);
  await balancerCard.getByLabel("Health interval seconds", { exact: true }).fill(values.healthIntervalSecs);
  await balancerCard.getByLabel("Health timeout seconds", { exact: true }).fill(values.healthTimeoutSecs);
  await balancerCard.getByLabel("Health max failures", { exact: true }).fill(values.healthMaxFailures);

  const profileRows = values.profiles || [];
  while ((await balancerCard.getByRole("button", { name: "Add Profile", exact: true }).count()) > 0 && (await balancerCard.locator(".advanced-config-inline-row").count()) < profileRows.length) {
    await balancerCard.getByRole("button", { name: "Add Profile", exact: true }).click();
    await pageWait();
  }
  const rows = balancerCard.locator(".advanced-config-inline-row");
  for (let index = 0; index < profileRows.length; index += 1) {
    const row = rows.nth(index);
    await row.getByLabel(`Profile ${index + 1} name`, { exact: true }).fill(profileRows[index].name);
    await row.getByLabel("Outbound tag", { exact: true }).fill(profileRows[index].outboundTag);
  }
}

async function resetRoutingCollections(page) {
  const routingPanel = page.locator(".editor-panel");

  while ((await routingPanel.locator(".advanced-config-subcard").filter({ hasText: "Rule " }).count()) > 0) {
    const cards = routingPanel.locator(".advanced-config-subcard").filter({ hasText: "Rule " });
    await cards
      .nth((await cards.count()) - 1)
      .locator(".section-editor-head")
      .first()
      .getByRole("button", { name: "Remove", exact: true })
      .click();
    await page.waitForTimeout(100);
  }

  while ((await routingPanel.locator(".advanced-config-subcard").filter({ hasText: "Balancer " }).count()) > 0) {
    const cards = routingPanel.locator(".advanced-config-subcard").filter({ hasText: "Balancer " });
    await cards
      .nth((await cards.count()) - 1)
      .locator(".section-editor-head")
      .first()
      .getByRole("button", { name: "Remove", exact: true })
      .click();
    await page.waitForTimeout(100);
  }
}

async function pageWait() {
  return new Promise((resolve) => setTimeout(resolve, 60));
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

async function applySettings(page, settings) {
  await nav(page, "Settings");
  await fillField(page, "Config path", settings.configPath);
  await fillField(page, "gRPC address", settings.grpcAddress);
  await fillField(page, "Public base URL", settings.publicBaseUrl);
  await fillField(page, "Subscription host", settings.subscriptionHost);
  await ensureSwitch(page, "Auto adaptive routing for enabled outbounds", settings.adaptiveRoutingEnabled);
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await waitForTopMessage(page, /Settings saved/i);
}

async function refreshPanel(page) {
  await page.reload({ waitUntil: "networkidle" });
  await ensureAuthenticated(page);
}

async function nav(page, name) {
  await page.getByRole("button", { name, exact: true }).click();
  await page.waitForTimeout(100);
}

async function ensureSwitch(page, label, checked) {
  const sw = page.getByRole("switch", { name: label, exact: true });
  const current = (await sw.getAttribute("aria-checked")) === "true";
  if (current !== checked) {
    await sw.click();
  }
}

async function ensureScopedSwitch(scope, label, checked) {
  const sw = scope.getByRole("switch", { name: label, exact: true });
  const current = (await sw.getAttribute("aria-checked")) === "true";
  if (current !== checked) {
    await sw.click();
  }
}

async function ensureSectionSwitch(page, checked) {
  const sw = editorPanel(page).locator(".summary-head [role='switch']").first();
  const current = (await sw.getAttribute("aria-checked")) === "true";
  if (current !== checked) {
    await sw.click();
  }
}

function editorPanel(page) {
  return page.locator(".editor-panel").first();
}

function escapeRegex(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function isLocatorDisabled(locator) {
  const disabled = await locator.getAttribute("disabled");
  const ariaDisabled = await locator.getAttribute("aria-disabled");
  return disabled !== null || ariaDisabled === "true";
}

async function fillField(page, label, value) {
  const field = page.getByLabel(label, { exact: true });
  await field.click({ delay: 20 });
  await field.fill(String(value));
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
    // The message may update too quickly to catch the intermediate state.
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

async function readConfig(configPath) {
  const raw = await readFile(configPath, "utf8");
  return JSON.parse(raw);
}

async function readConfigWhenReady(page, configPath, timeout = 12000) {
  const deadline = Date.now() + timeout;
  let lastError = "";
  while (Date.now() < deadline) {
    try {
      return await readConfig(configPath);
    } catch (error) {
      lastError = String(error instanceof Error ? error.message : error);
    }
    await page.waitForTimeout(250);
  }
  const strip = await page.locator(".strip-message").textContent().catch(() => "");
  throw new Error(`${lastError || "config file was not written"}${strip ? ` | strip: ${strip}` : ""}`);
}

async function readSections(page) {
  const db = openDb();
  return db
    .prepare("SELECT name, enabled, value, updated_at FROM config_sections ORDER BY name")
    .all()
    .map((row) => ({
      name: row.name,
      enabled: row.enabled === 1,
      value: row.value,
      updatedAt: row.updated_at
    }));
}

async function readOutbounds(page) {
  const db = openDb();
  return db
    .prepare("SELECT id, tag, protocol, enabled, settings, stream_settings, created_at, updated_at FROM outbounds ORDER BY id")
    .all()
    .map((row) => ({
      id: row.id,
      tag: row.tag,
      protocol: row.protocol,
      enabled: row.enabled === 1,
      settings: row.settings,
      streamSettings: row.stream_settings,
      createdAt: row.created_at,
      updatedAt: row.updated_at
    }));
}

async function resetConfigDbToDefaults() {
  const db = openDb();
  const ts = nowIso();
  db.exec("BEGIN");
  try {
    db.prepare("DELETE FROM outbounds").run();
    db.prepare(
      "INSERT INTO outbounds (tag, protocol, enabled, settings, stream_settings, created_at, updated_at) VALUES (:tag, :protocol, :enabled, :settings, :stream_settings, :created_at, :updated_at)"
    ).run({
      tag: "freedom",
      protocol: "freedom",
      enabled: 1,
      settings: "{}",
      stream_settings: "",
      created_at: ts,
      updated_at: ts
    });

    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "configPath", value: path.join(repoRoot, "black-ui", "data", "config.json") });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "grpcEnabled", value: "true" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "grpcAddress", value: "127.0.0.1:62789" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "firewallAutoOpen", value: "false" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "publicBaseUrl", value: "http://127.0.0.1:18080" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "subscriptionHost", value: "127.0.0.1" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "enforcementIntervalSeconds", value: "30" });
    db.prepare("UPDATE settings SET value=:value WHERE key=:key").run({ key: "adaptiveRoutingEnabled", value: "false" });

    const sections = [
      ["log", 1, "{\"level\":\"info\",\"json\":false}"],
      ["routing", 1, "{\"rules\":[{\"outboundTag\":\"freedom\"}]}"],
      ["dns", 0, "{\"servers\":[]}"],
      [
        "tun",
        0,
        "{\"name\":\"blackwire-tun\",\"address\":\"198.18.0.1\",\"netmask\":\"255.255.0.0\",\"mtu\":1500,\"bypass_mark\":4660,\"redirect_port\":7890,\"dns_port\":5300}"
      ],
      ["limits", 0, "{}"],
      ["stats", 0, "{}"],
      ["api", 1, "{\"listen\":\"127.0.0.1:62789\"}"],
      ["metricsAddr", 0, "\"127.0.0.1:9090\""],
      ["profile", 0, "\"compat\""],
      ["fast", 0, "{\"strictProduction\":true,\"pool\":\"disabled\",\"splice\":\"adaptive\"}"]
    ];

    db.prepare("DELETE FROM config_sections").run();
    const insertSection = db.prepare(
      "INSERT INTO config_sections (name, enabled, value, updated_at) VALUES (:name, :enabled, :value, :updated_at)"
    );
    for (const [name, enabled, value] of sections) {
      insertSection.run({ name, enabled, value, updated_at: ts });
    }

    db.exec("COMMIT");
  } catch (error) {
    db.exec("ROLLBACK");
    throw error;
  }
}

async function updateSection(page, name, enabled, value) {
  const db = openDb();
  db.prepare(
    "UPDATE config_sections SET enabled=:enabled, value=:value, updated_at=:updated_at WHERE name=:name"
  ).run({ enabled: enabled ? 1 : 0, value, updated_at: nowIso(), name });
}

async function restoreOriginalSections(page, sections) {
  const db = openDb();
  const update = db.prepare("UPDATE config_sections SET enabled=:enabled, value=:value, updated_at=:updated_at WHERE name=:name");
  for (const section of sections) {
    update.run({ enabled: section.enabled ? 1 : 0, value: section.value, updated_at: nowIso(), name: section.name });
  }
}

async function createOutbound(page, outbound) {
  const db = openDb();
  db.prepare("DELETE FROM outbounds WHERE tag=:tag").run({ tag: outbound.tag });
  db.prepare(
    "INSERT INTO outbounds (tag, protocol, enabled, settings, stream_settings, created_at, updated_at) VALUES (:tag, :protocol, :enabled, :settings, :stream_settings, :created_at, :updated_at)"
  ).run({
    tag: outbound.tag,
    protocol: outbound.protocol,
    enabled: outbound.enabled ? 1 : 0,
    settings: outbound.settings ?? "{}",
    stream_settings: outbound.streamSettings ?? "",
    created_at: nowIso(),
    updated_at: nowIso()
  });
}

async function updateOutbound(page, outbound) {
  const db = openDb();
  db.prepare(
    "UPDATE outbounds SET tag=:tag, protocol=:protocol, enabled=:enabled, settings=:settings, stream_settings=:stream_settings, updated_at=:updated_at WHERE id=:id"
  ).run({
    tag: outbound.tag,
    protocol: outbound.protocol,
    enabled: outbound.enabled ? 1 : 0,
    settings: outbound.settings ?? "{}",
    stream_settings: outbound.streamSettings ?? "",
    updated_at: nowIso(),
    id: outbound.id
  });
}

async function deleteOutbound(page, id) {
  const db = openDb();
  db.prepare("DELETE FROM outbounds WHERE id=:id").run({ id });
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

async function ensureQaOutbounds(page) {
  await purgeQaOutbounds();
  for (const tag of qaOutboundTags) {
    await createOutbound(page, {
      tag,
      protocol: "freedom",
      enabled: true,
      settings: "{}",
      streamSettings: ""
    });
  }
  restorationNotes.push(`QA outbounds seeded: ${qaOutboundTags.join(", ")}.`);
}

async function disableOriginalEnabledOutbounds(page, outbounds) {
  for (const outbound of outbounds.filter((item) => item.enabled)) {
    await updateOutbound(page, {
      ...outbound,
      enabled: false
    });
  }
  restorationNotes.push("Original enabled outbounds were temporarily disabled so the QA outbounds would be first in template order.");
}

async function restoreOriginalOutbounds(page, outbounds) {
  for (const outbound of outbounds) {
    await updateOutbound(page, {
      ...outbound,
      enabled: outbound.enabled
    });
  }
}

async function deleteQaOutbounds(page, tags) {
  await purgeQaOutbounds();
}

async function purgeQaOutbounds() {
  const db = openDb();
  db.prepare("DELETE FROM outbounds WHERE tag LIKE 'qa-%'").run();
}

function openDb() {
  return new DatabaseSync(path.join(repoRoot, "black-ui", "data", "black-ui.db"));
}

function nowIso() {
  return new Date().toISOString();
}

function buildReport() {
  return {
    generatedAt: new Date().toISOString(),
    panelUrl: uiUrl,
    configPath: runConfigPath,
    counts: {
      passed: results.filter((item) => item.status === "PASS").length,
      skipped: skippedCases.length,
      failed: results.filter((item) => item.status === "FAILED").length
    },
    results,
    skippedCases,
    restorationNotes,
    failure: failure ? String(failure) : ""
  };
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
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

async function writeMarkdownReport(report) {
  const lines = [];
  lines.push("# Advanced Config Panel QA Result");
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
  if (report.results.length === 0) {
    lines.push("- No cases ran.");
  } else {
    for (const item of report.results) {
      lines.push(`- ${item.status}: ${item.name} - ${item.details}`);
    }
  }
  lines.push("");
  lines.push("## Skipped Cases");
  if (report.skippedCases.length === 0) {
    lines.push("- None.");
  } else {
    for (const item of report.skippedCases) {
      lines.push(`- ${item.name}: ${item.reason}`);
    }
  }
  lines.push("");
  lines.push("## Restoration");
  if (report.restorationNotes.length === 0) {
    lines.push("- No restoration notes were recorded.");
  } else {
    for (const item of report.restorationNotes) {
      lines.push(`- ${item}`);
    }
  }
  lines.push("");
  lines.push("## Notes");
  lines.push("- The live panel was exercised through the structured Advanced Config editor and raw JSON fallback sections.");
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

async function navAndRefresh(page, sectionName) {
  await refreshPanel(page);
  await nav(page, sectionName);
}
