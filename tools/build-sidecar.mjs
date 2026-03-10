#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = process.env.CARGO_TARGET_DIR
  ? path.resolve(process.env.CARGO_TARGET_DIR)
  : path.join(root, "target");
const sidecarDir = path.join(root, "tauri-workspace", "src-tauri", "binaries");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "pipe",
    encoding: "utf8",
  });
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    process.exit(result.status ?? 1);
  }
  return result.stdout.trim();
}

function hostTriple() {
  const version = run("rustc", ["-vV"]);
  const hostLine = version
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.startsWith("host: "));
  if (!hostLine) {
    throw new Error("Unable to determine rustc host triple");
  }
  return hostLine.slice("host: ".length).trim();
}

const triple = hostTriple();
const exeSuffix = triple.includes("windows") ? ".exe" : "";

run("cargo", ["build", "--release", "--bin", "nearxd", "--manifest-path", path.join(root, "Cargo.toml")]);

mkdirSync(sidecarDir, { recursive: true });

const source = path.join(targetDir, "release", `nearxd${exeSuffix}`);
const destination = path.join(sidecarDir, `nearxd-${triple}${exeSuffix}`);
copyFileSync(source, destination);
chmodSync(destination, 0o755);

process.stdout.write(`${destination}\n`);
