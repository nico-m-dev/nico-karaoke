import { spawn } from "node:child_process";
import net from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const API_PORT = 8770;
const VITE_URL = "http://localhost:5173/";
const children = [];

function portIsFree(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => server.close(() => resolve(true)));
    server.listen(port, "127.0.0.1");
  });
}

async function waitFor(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      // Vite is still starting.
    }
    await delay(200);
  }
  throw new Error(`Vite did not become ready at ${url}`);
}

function stop(exitCode) {
  for (const child of children) {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill("SIGTERM");
    }
  }
  process.exit(exitCode);
}

process.on("SIGINT", () => stop(130));
process.on("SIGTERM", () => stop(143));

if (!(await portIsFree(API_PORT))) {
  console.error(
    `Port ${API_PORT} is busy. Stop the other server first; the Vite proxy targets it.`,
  );
  process.exit(1);
}

const vite = spawn("npm", ["run", "dev", "--prefix", "web"], {
  stdio: "inherit",
  env: process.env,
});
children.push(vite);
vite.on("exit", (code) => {
  if (code !== 0) {
    console.error("Vite stopped before the desktop app closed.");
    stop(code ?? 1);
  }
});

try {
  await waitFor(VITE_URL, 20_000);
} catch (error) {
  console.error(error.message);
  stop(1);
}

const app = spawn("cargo", ["run", "-p", "nico-karaoke-desktop"], {
  stdio: "inherit",
  env: {
    ...process.env,
    NICO_KARAOKE_WEBVIEW_URL: VITE_URL,
  },
});
children.push(app);
app.on("exit", (code) => stop(code ?? 0));
