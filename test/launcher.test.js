#!/usr/bin/env node

const assert = require('node:assert');
const { spawnSync, spawn } = require('node:child_process');
const { existsSync, readFileSync } = require('node:fs');
const { join, resolve } = require('node:path');

const cliJs = join(__dirname, '..', 'bin', 'cli.js');
const root = join(__dirname, '..');
const targetRoot = process.env.CARGO_TARGET_DIR
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : join(root, 'target');
const binary = join(
  targetRoot,
  'debug',
  process.platform === 'win32' ? 'grok-bridge.exe' : 'grok-bridge'
);

function test(name, fn) {
  try {
    fn();
    console.log(`✓ ${name}`);
  } catch (error) {
    console.error(`✗ ${name}`);
    console.error(error.message);
    process.exit(1);
  }
}

if (!existsSync(binary)) {
  console.log(`⚠ Skipping launcher tests: ${binary} not found (run 'cargo build' first)`);
  process.exit(0);
}

test('resolves binary via environment override', () => {
  const result = spawnSync(process.execPath, [cliJs, '--version'], {
    env: { ...process.env, GROK_BRIDGE_BINARY: binary },
    encoding: 'utf8',
  });
  assert(result.status === 0, `should succeed with GROK_BRIDGE_BINARY override: ${result.stderr}`);
});

test('errors when GROK_BRIDGE_BINARY does not exist', () => {
  const result = spawnSync(process.execPath, [cliJs, '--version'], {
    env: { ...process.env, GROK_BRIDGE_BINARY: '/nonexistent/grok-bridge' },
    encoding: 'utf8',
  });
  assert(result.status !== 0, 'should fail with nonexistent override');
  assert(result.stderr.includes('not found'), 'should mention not found');
});

test('passes stdin when not a TTY', () => {
  const payload = '{"hook":"test-stdin-forwarding"}';
  const result = spawnSync(process.execPath, [cliJs, 'hooks', 'status'], {
    input: payload,
    env: { ...process.env, GROK_BRIDGE_BINARY: binary },
    encoding: 'utf8',
  });
  assert(result.status === 0, `stdin forwarding failed: ${result.stderr}`);
});

test('passes argv to binary', () => {
  const result = spawnSync(process.execPath, [cliJs, 'list'], {
    env: { ...process.env, GROK_BRIDGE_BINARY: binary },
    encoding: 'utf8',
  });
  assert(result.status === 0 || result.stderr.includes('No server'), 'should pass argv to binary');
});

test('argv invocation does not block when stdin remains open', (done) => {
  const child = spawn(process.execPath, [cliJs, 'list'], {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, GROK_BRIDGE_BINARY: binary },
  });
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', (chunk) => { stderr += chunk; });
  const timeout = setTimeout(() => {
    child.kill();
    throw new Error('argv invocation blocked while stdin remained open');
  }, 3000);
  child.on('exit', (status) => {
    clearTimeout(timeout);
    assert(status === 0 || stderr.includes('No server'), `should exit quickly: ${stderr}`);
  });
});

console.log('\nAll launcher tests passed.\n');
