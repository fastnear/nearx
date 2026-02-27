import fs from 'fs';
import os from 'os';
import path from 'path';
import { fileURLToPath } from 'url';
import { spawn, spawnSync } from 'child_process';
import { Builder, By, Capabilities, Key, until } from 'selenium-webdriver';
import { expect } from 'chai';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..'); // e2e-tests -> nearx root

const appDir = path.resolve(repoRoot, 'tauri-workspace');
const binaryName = process.platform === 'win32' ? 'nearx-tauri.exe' : 'nearx-tauri';
const applicationCandidates = [
  path.resolve(appDir, 'target', 'debug', binaryName),
  path.resolve(appDir, 'src-tauri', 'target', 'debug', binaryName)
];
let application = applicationCandidates[0];

let driver;
let tauriDriver;
let exit = false;

async function waitForHashContains(fragment, timeoutMs = 15000) {
  await driver.wait(async () => {
    const hash = await driver.executeScript(() => window.location.hash || '');
    return String(hash).includes(fragment);
  }, timeoutMs, `Expected location hash to include '${fragment}'`);
}

async function findSearchInput() {
  return driver.wait(
    until.elementLocated(By.css('input[placeholder="Search tx, block, or account"]')),
    20000,
    'Search input did not appear'
  );
}

async function submitSearch(value) {
  const input = await findSearchInput();
  await input.clear();
  await input.sendKeys(value, Key.ENTER);
}

before(async function () {
  this.timeout(180000); // 3 minutes for build + driver startup

  console.log('Building Tauri app with e2e features...');
  console.log('App directory:', appDir);
  console.log('Candidate binary paths:', applicationCandidates);

  const buildResult = spawnSync(
    'cargo',
    ['tauri', 'build', '--debug', '--no-bundle', '--features', 'e2e'],
    {
      cwd: appDir,
      stdio: 'inherit',
      shell: true
    }
  );

  if (buildResult.error) {
    console.error('Build failed:', buildResult.error);
    throw buildResult.error;
  }

  application =
    applicationCandidates.find((candidate) => fs.existsSync(candidate)) ?? applicationCandidates[0];
  console.log('Resolved binary path:', application);

  console.log('Starting tauri-driver...');

  const tauriDriverPath = path.resolve(os.homedir(), '.cargo', 'bin', 'tauri-driver');
  tauriDriver = spawn(tauriDriverPath, [], {
    stdio: [null, process.stdout, process.stderr]
  });

  tauriDriver.on('error', (e) => {
    console.error('tauri-driver error:', e);
    process.exit(1);
  });

  tauriDriver.on('exit', (code) => {
    if (!exit) {
      console.error('tauri-driver exited unexpectedly with code:', code);
      process.exit(1);
    }
  });

  await new Promise((resolve) => setTimeout(resolve, 2000));

  console.log('Connecting WebDriver...');

  const caps = new Capabilities();
  caps.set('tauri:options', { application });
  caps.setBrowserName('wry');

  driver = await new Builder()
    .withCapabilities(caps)
    .usingServer('http://127.0.0.1:4444/')
    .build();

  console.log('WebDriver connected successfully');

  await driver.wait(async () => {
    const state = await driver.executeScript(() => document.readyState);
    return state === 'complete';
  }, 15000, 'Document did not finish loading');
});

after(async function () {
  exit = true;
  console.log('Cleaning up...');

  if (driver) {
    try {
      await driver.quit();
      console.log('WebDriver quit');
    } catch (error) {
      console.error('Error quitting driver:', error);
    }
  }

  if (tauriDriver) {
    tauriDriver.kill();
    console.log('tauri-driver killed');
  }
});

describe('NEARx Explorer Desktop (Tauri) – E2E Smoke Tests', function () {
  this.timeout(60000);

  it('renders the explorer shell', async () => {
    const brand = await driver.wait(
      until.elementLocated(By.xpath("//a[contains(., 'NEAR Rocks')]")),
      20000,
      'Brand link did not render'
    );

    expect(await brand.getText()).to.contain('NEAR Rocks');

    const search = await findSearchInput();
    expect(search).to.not.equal(null);
  });

  it('navigates to block detail via search', async () => {
    await submitSearch('123456');
    await waitForHashContains('/block/123456');
  });

  it('navigates to account detail via search', async () => {
    await submitSearch('alice.near');
    await waitForHashContains('/account/alice.near');
  });

  it('falls back to home route for unsupported paths', async () => {
    await driver.executeScript(() => {
      window.location.hash = '#/this/route/does/not/exist';
    });

    await driver.wait(async () => {
      const hash = await driver.executeScript(() => window.location.hash || '');
      return hash === '#/' || hash === '#';
    }, 15000, 'Unsupported route did not redirect to home');

    const heading = await driver.wait(
      until.elementLocated(By.xpath("//h1[contains(., 'Latest Blocks')]")),
      20000,
      'Home heading did not render after fallback'
    );

    expect(await heading.getText()).to.contain('Latest Blocks');
  });
});
