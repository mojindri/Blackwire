import { spawn } from "node:child_process";
import { mkdir, rm, writeFile } from "node:fs/promises";

const repoRoot = new URL("../../..", import.meta.url).pathname;
const frontendDir = `${repoRoot}/black-ui/frontend`;
const scriptsDir = `${frontendDir}/scripts`;
const workDir = "/tmp/black-ui-qa-all";
const uiData = `${workDir}/ui-data`;
const bwConfig = `${workDir}/blackwire.json`;
const uiBase = "http://127.0.0.1:18096";
const grpcAddress = "127.0.0.1:26296";
const processes = [];

async function main() {
  await run("node", [`${scriptsDir}/qa-flow.mjs`], { cwd: frontendDir });

  await rm(workDir, { recursive: true, force: true });
  await mkdir(uiData, { recursive: true });
  await writeFile(
    bwConfig,
    JSON.stringify(
      {
        api: { listen: grpcAddress },
        log: { level: "info", json: false },
        inbounds: [{ tag: "seed-socks", listen: "127.0.0.1", port: 26297, protocol: "socks" }],
        outbounds: [{ tag: "freedom", protocol: "freedom", settings: {} }],
        routing: { rules: [{ outboundTag: "freedom" }] }
      },
      null,
      2
    )
  );

  await run("cargo", ["run", "-q", "-p", "blackwire", "--", "test", "-c", bwConfig]);
  await run("npm", ["exec", "--", "vite", "build"], { cwd: frontendDir });

  processes.push(spawn("cargo", ["run", "-q", "-p", "blackwire", "--", "run", "-c", bwConfig], { cwd: repoRoot }));
  await waitForPort(26296);
  processes.push(
    spawn("cargo", ["run", "-q", "-p", "black-ui-server"], {
      cwd: repoRoot,
      env: { ...process.env, BLACK_UI_DATA_DIR: uiData, BLACK_UI_LISTEN: "127.0.0.1:18096" }
    })
  );
  await waitForHttp(`${uiBase}/api/status`);

  const commonArgs = [
    "--ui-url",
    uiBase,
    "--grpc",
    grpcAddress,
    "--public-base-url",
    uiBase,
    "--subscription-host",
    "127.0.0.1",
    "--no-doc-report"
  ];

  await run("node", [`${scriptsDir}/qa-inbounds-structured.mjs`, ...commonArgs, "--config-path", `${workDir}/inbounds-config.json`], {
    cwd: frontendDir,
    env: { ...process.env, BLACK_UI_DATA_DIR: uiData }
  });
  await run("node", [`${scriptsDir}/qa-outbounds-structured.mjs`, ...commonArgs, "--config-path", `${workDir}/outbounds-config.json`], {
    cwd: frontendDir,
    env: { ...process.env, BLACK_UI_DATA_DIR: uiData }
  });
  await run("node", [`${scriptsDir}/qa-advanced-config-structured.mjs`, ...commonArgs, "--config-path", `${workDir}/advanced-config.json`], {
    cwd: frontendDir,
    env: { ...process.env, BLACK_UI_DATA_DIR: uiData }
  });

  console.log("black-ui full QA passed");
}

async function run(command, args, options = {}) {
  const child = spawn(command, args, { cwd: repoRoot, stdio: "inherit", ...options });
  const code = await new Promise((resolve) => child.on("close", resolve));
  if (code !== 0) throw new Error(`${command} ${args.join(" ")} failed with ${code}`);
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

async function waitForPort(port) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const socket = await import("node:net").then(({ createConnection }) => createConnection({ host: "127.0.0.1", port }));
      await new Promise((resolve, reject) => {
        socket.once("connect", resolve);
        socket.once("error", reject);
      });
      socket.destroy();
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
  throw new Error(`timed out waiting for port ${port}`);
}

main()
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  })
  .finally(() => {
    for (const child of processes.reverse()) child.kill("SIGINT");
  });
