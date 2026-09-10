import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { readFile, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkSearch } from './check-search.mjs';
const root = fileURLToPath(new URL('../..', import.meta.url));
const temporary = await mkdtemp(join(tmpdir(), 'flix-browser-check-'));
const media = join(temporary, 'sample.mp4');
const control = join(temporary, 'control');
let host;
let hostError;
let browser;
let launchUrl;
let origin;
let access;

function command(program, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(program, args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], ...options });
    let output = '';
    child.stdout.on('data', bytes => { output = (output + bytes).slice(-8192); });
    child.stderr.on('data', bytes => { output = (output + bytes).slice(-8192); });
    child.on('error', reject);
    child.on('exit', code => code === 0 ? resolve() : reject(new Error(`${program} failed: ${output}`)));
  });
}

async function waitForControl() {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline && host.exitCode === null && !hostError) {
    try { return await readFile(control, 'utf8'); } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('The Rust browser test host did not start.');
}

try {
 console.log('Build the web app and generate a sample video.');
 const npmCli = process.env.npm_execpath;
 if (!npmCli) throw new Error('Run this check with npm run test:browser.');
 await command(process.execPath, [npmCli, '--prefix', 'web', 'run', 'build']);
 await writeFile(join(root, 'web/dist/.gitkeep'), '');
 await command('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=24', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '30', '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-movflags', '+faststart', media]);
 console.log('Start the Rust host and check browser playback.');
 host = spawn('cargo', ['test', '--lib', '--features', 'web-ui', 'browser_smoke_host', '--', '--ignored'], {
   cwd: root, stdio: ['ignore', 'ignore', 'pipe'],
   env: { ...process.env, FLIX_BROWSER_TEST_MEDIA: media, FLIX_BROWSER_TEST_CONTROL: control },
 });
 host.stderr.resume();
 host.on('error', error => { hostError = error; });
 launchUrl = (await waitForControl()).trim();
 origin = new URL(launchUrl).origin;
 access = new URL(launchUrl).searchParams.get('token');
 browser = await chromium.launch({ executablePath: process.env.FLIX_TEST_CHROME, headless: true });
 const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
 const errors = [];
 page.on('pageerror', error => errors.push(error.message));
 await page.route('**/api/activation', route => route.fulfill({ json: { is_activated: true } }));
 await page.goto(launchUrl);
 const panel = page.getByRole('region', { name: 'Browser player' });
 await panel.waitFor();
 const video = panel.locator('video');
 if (await video.evaluate(element => element.paused)) await panel.getByRole('button', { name: 'Play', exact: true }).click();
 await page.waitForFunction(() => document.querySelector('video')?.getVideoPlaybackQuality().totalVideoFrames > 2);
 const initial = await video.evaluate(element => ({ time: element.currentTime, width: element.videoWidth, tracks: element.textTracks.length }));
 assert.equal(initial.width, 640);
 assert.equal(initial.tracks, 1);
 await page.waitForFunction(() => document.querySelector('video')?.textTracks[0]?.cues?.length > 0);
 await panel.getByRole('button', { name: 'Pause', exact: true }).click();
 await page.waitForFunction(() => document.querySelector('video')?.paused);
 await panel.getByRole('button', { name: 'Forward 10 seconds' }).click();
 await page.waitForFunction(() => document.querySelector('video')?.currentTime >= 10);
 await panel.getByRole('button', { name: 'Back 10 seconds' }).click();
 assert.ok(await video.evaluate(element => element.currentTime < 5));

 await page.setViewportSize({ width: 390, height: 900 });
 assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'Mobile layout must fit the viewport.');

 await panel.getByRole('button', { name: 'Forward 10 seconds' }).click();
 await page.waitForFunction(async () => {
   const response = await fetch('/api/status', { headers: { 'X-Flix-Token': sessionStorage.getItem('flix_token') } });
   const { state } = await response.json();
   return state.state === 'browser' && state.phase === 'paused' && state.position_ms >= 10000;
 });
 await page.reload();
 await panel.waitFor();
 await page.waitForFunction(() => document.querySelector('video')?.currentTime >= 10);
 assert.equal(await video.evaluate(element => element.paused), true, 'Reload must preserve pause.');
 await panel.getByRole('button', { name: 'Play', exact: true }).click();
 await page.waitForFunction(() => document.querySelector('video')?.currentTime > 1);
 await page.getByRole('button', { name: 'Reader', exact: true }).click();
 assert.equal(await video.evaluate(element => element.paused), false);
 await panel.getByRole('button', { name: 'Stop', exact: true }).click();
 await panel.waitFor({ state: 'detached' });
 const state = await page.evaluate(async () => (await (await fetch('/api/status', { headers: { 'X-Flix-Token': sessionStorage.getItem('flix_token') } })).json()).state);
 assert.equal(state.state, 'idle');
 await checkSearch(page);
 assert.equal(errors.length, 0, 'The browser must not report script errors.');
 console.log('PASS: decoded frames, English subtitle cues, pause, resume, seek, reload, navigation, mobile layout, and Stop.');
 } catch (error) {
 const message = String(error.stack || error.message).replace(/([?&](?:token|t)=)[^\s&"']+/g, '$1[redacted]');
 console.error(message);
 process.exitCode = 1;
} finally {
 await browser?.close();
 if (origin && access) await fetch(origin + '/api/quit', { method: 'POST', headers: { 'X-Flix-Token': access }, signal: AbortSignal.timeout(5000) }).catch(() => {});
 if (host && host.exitCode === null) {
   await Promise.race([new Promise(resolve => host.once('exit', resolve)), new Promise(resolve => setTimeout(resolve, 5000))]);
   if (host.exitCode === null) host.kill('SIGTERM');
 }
 await rm(temporary, { recursive: true, force: true });
}
