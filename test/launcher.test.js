#!/usr/bin/env node

const assert = require('node:assert');
const { spawnSync } = require('node:child_process');
const { existsSync, mkdirSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');

const cliJs = join(__dirname, '..', 'bin', 'cli.js');

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

test('resolves binary via environment override', () => {
  const tempBin = join(tmpdir(), `grok-bridge-test-${Date.now()}`);
  writeFileSync(tempBin, '#!/bin/sh\necho "mock version 1.0.0"\n', { mode: 0o755 });
  const result = spawnSync(process.execPath, [cliJs, '--version'], {
    env: { ...process.env, GROK_BRIDGE_BINARY: tempBin },
    encoding: 'utf8',
  });
  assert(result.stdout.includes('mock version'), 'should use GROK_BRIDGE_BINARY override');
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
  const tempBin = join(tmpdir(), `grok-bridge-echo-${Date.now()}`);
  if (process.platform === 'win32') {
    writeFileSync(tempBin + '.cmd', '@echo off\nfindstr ".*"\n');
  } else {
    writeFileSync(tempBin, '#!/bin/sh\ncat\n', { mode: 0o755 });
  }
  const actualBin = process.platform === 'win32' ? tempBin + '.cmd' : tempBin;
  const result = spawnSync(process.execPath, [cliJs], {
    input: 'test-hook-payload',
    env: { ...process.env, GROK_BRIDGE_BINARY: actualBin },
    encoding: 'utf8',
  });
  assert(result.stdout.includes('test-hook-payload'), 'should pass stdin to binary');
});

test('passes argv to binary', () => {
  const tempBin = join(tmpdir(), `grok-bridge-args-${Date.now()}`);
  if (process.platform === 'win32') {
    writeFileSync(tempBin + '.cmd', '@echo off\necho %*\n');
  } else {
    writeFileSync(tempBin, '#!/bin/sh\necho "$@"\n', { mode: 0o755 });
  }
  const actualBin = process.platform === 'win32' ? tempBin + '.cmd' : tempBin;
  const result = spawnSync(process.execPath, [cliJs, 'list', '--session', 'gbt-1'], {
    env: { ...process.env, GROK_BRIDGE_BINARY: actualBin },
    encoding: 'utf8',
  });
  assert(result.stdout.includes('list'), 'should pass argv to binary');
  assert(result.stdout.includes('gbt-1'), 'should pass all arguments');
});

console.log('\nAll launcher tests passed.\n');
