import { spawn } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { chromium } from "playwright";

const repoRoot = new URL("../../..", import.meta.url).pathname;
const workDir = "/tmp/black-ui-qa-flow";
const uiData = `${workDir}/ui-data`;
const uiBase = "http://127.0.0.1:18094";
const frontendDir = `${repoRoot}/black-ui/frontend`;
const processes = [];
const databaseUrl = process.env.BLACKWIRE_QA_DATABASE_URL;

async function main() {
  if (!databaseUrl) throw new Error("BLACKWIRE_QA_DATABASE_URL must point to an empty MySQL 8.4 database");
  await rm(workDir, { recursive: true, force: true });
  await mkdir(uiData, { recursive: true });
  const databaseEnv = { ...process.env, BLACKWIRE_DATABASE_URL: databaseUrl };
  await run("cargo", ["run", "-q", "-p", "blackwire", "--", "db", "init"], { env: databaseEnv });
  await run("cargo", ["run", "-q", "-p", "blackwire", "--", "db", "seed", "socks-local"], { env: databaseEnv });
  await run("npm", ["exec", "--", "vite", "build"], { cwd: frontendDir });
  processes.push(spawn("cargo", ["run", "-q", "-p", "blackwire", "--", "run"], { cwd: repoRoot, env: databaseEnv }));
  processes.push(
    spawn("cargo", ["run", "-q", "-p", "black-ui-server"], {
      cwd: repoRoot,
      env: { ...databaseEnv, BLACK_UI_DATA_DIR: uiData, BLACK_UI_LISTEN: "127.0.0.1:18094" }
    })
  );
  await waitForHttp(`${uiBase}/api/status`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 980 },
    permissions: ["clipboard-read", "clipboard-write"],
    origin: uiBase
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  const consoleMessages = [];
  page.on("console", (msg) => {
    if (["error", "warning"].includes(msg.type())) consoleMessages.push(`${msg.type()}: ${msg.text()}`);
  });
  page.on("pageerror", (err) => consoleMessages.push(`pageerror: ${err.message}`));

  await page.goto(uiBase, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Create admin", exact: true }).waitFor();
  await page.getByLabel("Username", { exact: true }).fill("admin");
  await page.getByLabel("Password", { exact: true }).fill("password123");
  await page.getByRole("button", { name: "Create and enter", exact: true }).click();
  await page.getByRole("heading", { name: "Users", exact: true }).waitFor();

  await nav(page, "Settings");
  await page.getByLabel("Public base URL", { exact: true }).fill(uiBase);
  await page.getByLabel("Subscription host", { exact: true }).fill("127.0.0.1");
  await page.getByRole("button", { name: "Save Settings", exact: true }).click();
  await strip(page, /Settings saved/);
  await waitForIdle(page);

  await addInbound(page, "qa-main", "26320");
  await addUser(page, "qa@example.com", "qa-main");
  const userRow = page.locator("tr", { hasText: "qa@example.com" });
  await userRow.getByRole("button", { name: "Copy subscription content", exact: true }).click();
  await page.getByText("Copied", { exact: true }).waitFor();

  await nav(page, "Inbounds");
  await addInbound(page, "qa-delete", "26321");
  await page.getByRole("button", { name: /qa-delete/ }).click();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "qa-delete", exact: true }).waitFor({ state: "detached" });

  await nav(page, "Runtime");
  await page.getByText(/Revision history/i).waitFor();

  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "Users", exact: true }).waitFor();
  await nav(page, "Settings");
  await page.getByRole("heading", { name: "Settings", exact: true }).waitFor();

  await browser.close();
  const relevantConsole = consoleMessages.filter((message) => !message.includes("401"));
  if (relevantConsole.length) throw new Error(`console issues: ${relevantConsole.join("; ")}`);
  console.log("black-ui QA flow passed");
}

async function addInbound(page, tag, port) {
  await nav(page, "Inbounds");
  await page.getByRole("button", { name: "New Inbound", exact: true }).click();
  await page.getByLabel("Tag", { exact: true }).fill(tag);
  await page.getByLabel("Listen address", { exact: true }).fill("127.0.0.1");
  await page.getByLabel("Port", { exact: true }).fill(port);
  await page.getByLabel("Transport", { exact: true }).selectOption("tcp");
  const saveButton = page.getByRole("button", { name: "Save revision", exact: true });
  if (await saveButton.isDisabled()) {
    const inlineErrors = await page.locator(".inline-error, .field-error").allTextContents();
    throw new Error(`Save Inbound is disabled for ${tag}: ${inlineErrors.join(" | ") || "no validation message found"}`);
  }
  await saveButton.click();
  await page.getByRole("button", { name: tag, exact: true }).waitFor();
}

async function addUser(page, email, inboundLabel) {
  await nav(page, "Users");
  await page.getByRole("button", { name: "Add User", exact: true }).click();
  await page.getByLabel("Email", { exact: true }).fill(email);
  const inboundSelect = page.getByLabel("Inbound", { exact: true });
  const inboundValue = await inboundSelect.locator("option").evaluateAll(
    (options, prefix) => options.find((option) => option.textContent?.trim().startsWith(prefix))?.value ?? null,
    inboundLabel
  );
  if (!inboundValue) throw new Error(`inbound option not found for ${inboundLabel}`);
  await inboundSelect.selectOption(inboundValue);
  await page.getByRole("button", { name: "Generate", exact: true }).click();
  await page.waitForFunction(() => Array.from(document.querySelectorAll("input")).some((input) => input.value.includes("-")));
  await page.getByRole("button", { name: "Save revision", exact: true }).click();
  await page.locator("tr", { hasText: email }).waitFor({ timeout: 30000 });
}

async function nav(page, name) {
  await page.getByRole("button", { name, exact: true }).click();
  await page.getByRole("heading", { name, exact: true }).waitFor();
}

async function strip(page, pattern) {
  await page.waitForFunction((source) => new RegExp(source, "i").test(document.querySelector(".strip-message")?.textContent ?? ""), pattern.source);
}

async function waitForIdle(page) {
  await page.getByRole("button", { name: "Refresh", exact: true }).waitFor({ state: "visible" });
  await page.waitForFunction(
    () => {
      const refresh = Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.trim() === "Refresh");
      return refresh instanceof HTMLButtonElement && !refresh.disabled;
    },
    undefined,
    { timeout: 30000 }
  );
}

async function waitForHttp(url) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function run(command, args, options = {}) {
  const child = spawn(command, args, { cwd: repoRoot, stdio: "inherit", ...options });
  const code = await new Promise((resolve) => child.on("close", resolve));
  if (code !== 0) throw new Error(`${command} ${args.join(" ")} failed with ${code}`);
}

main()
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  })
  .finally(() => {
    for (const child of processes.reverse()) child.kill("SIGINT");
  });
