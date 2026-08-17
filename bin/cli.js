#!/usr/bin/env node

const { spawnSync } = require('node:child_process');
const { existsSync, readFileSync } = require('node:fs');
const { dirname, join } = require('node:path');

const TARGETS = {
  'win32:x64': ['@asural/grok-bridge-win32-x64', 'grok-bridge.exe'],
  'win32:arm64': ['@asural/grok-bridge-win32-arm64', 'grok-bridge.exe'],
  'linux:x64': ['@asural/grok-bridge-linux-x64', 'grok-bridge'],
  'linux:arm64': ['@asural/grok-bridge-linux-arm64', 'grok-bridge'],
  'darwin:x64': ['@asural/grok-bridge-darwin-x64', 'grok-bridge'],
  'darwin:arm64': ['@asural/grok-bridge-darwin-arm64', 'grok-bridge'],
};

(() => {
  try {
    const binary = resolveBinary();
    const interactive = process.stdin.isTTY && process.stdout.isTTY;
    const args = process.argv.slice(2);
    const result = spawnSync(binary, args, interactive
      ? { stdio: 'inherit', env: process.env, windowsHide: true }
      : args.length > 0
      ? { stdio: ['ignore', 'inherit', 'inherit'], env: process.env, windowsHide: true }
      : {
          input: readHookInput(),
          stdio: ['pipe', 'inherit', 'inherit'],
          env: process.env,
          windowsHide: true,
        });
    if (result.error) throw result.error;
    process.exit(result.status ?? 0);
  } catch (err) {
    console.error(`[grok-bridge] ${err.message}`);
    process.exit(1);
  }
})();

function resolveBinary() {
  if (process.env.GROK_BRIDGE_BINARY) {
    if (!existsSync(process.env.GROK_BRIDGE_BINARY)) {
      throw new Error(`GROK_BRIDGE_BINARY not found: ${process.env.GROK_BRIDGE_BINARY}`);
    }
    return process.env.GROK_BRIDGE_BINARY;
  }

  const target = TARGETS[`${process.platform}:${process.arch}`];
  if (!target) {
    throw new Error(`unsupported platform: ${process.platform}:${process.arch}`);
  }
  const [packageName, executable] = target;
  try {
    const packageRoot = dirname(require.resolve(`${packageName}/package.json`));
    const binary = join(packageRoot, 'bin', executable);
    if (!existsSync(binary)) throw new Error(`binary not found: ${binary}`);
    return binary;
  } catch (error) {
    throw new Error(
      `native package ${packageName} is unavailable (${error.message}). ` +
      'Reinstall from the official npm registry.'
    );
  }
}

function readHookInput() {
  return process.stdin.isTTY ? undefined : readFileSync(0);
}
