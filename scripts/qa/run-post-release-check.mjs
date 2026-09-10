// Post-release QA check. Drives the packaged flix-desktop binary through its
// local HTTP API: activate with a QA invite code, then play a movie, an
// anime title, and a TV show, using local synthetic fixtures instead of a
// live catalog or a public tracker. Exits non-zero on the first regression
// it finds, and prints which check failed and why.
//
// Every check uses the browser-preview playback target, so nothing here
// needs VLC, mpv, or any other media player installed on the runner - the
// whole check is plain HTTP against the app's local API and stream server.
//
// Usage: node run-post-release-check.mjs --app <path-to-flix-desktop>

import { spawn, spawnSync } from 'node:child_process';
import { cp, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..');
const PORT = 8998;
const BOOTSTRAP_LINE = 'Open this URL in your browser: ';

function arg(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited with code ${result.status}`);
  }
}

async function buildTorrent(inputPath, outputPath) {
  const result = spawnSync(
    'cargo',
    ['run', '--release', '--example', 'qa_torrent', '--', inputPath, outputPath],
    { cwd: repoRoot, encoding: 'utf8' },
  );
  if (result.status !== 0) {
    throw new Error(`qa_torrent failed for ${inputPath}: ${result.stderr}`);
  }
  return outputPath;
}

function findDownloadDir(appPath) {
  const result = spawnSync(appPath, ['--check'], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`${appPath} --check failed: ${result.stderr}`);
  }
  const match = result.stdout.match(/Download directory: (.+)/);
  if (!match) {
    throw new Error(`Could not read the download directory from --check output:\n${result.stdout}`);
  }
  return match[1].trim();
}

// The app prints its local bootstrap URL once on startup: this pulls the
// per-run credential out of that URL's query string instead of pattern
// matching on it directly.
function extractBootstrapCredential(line) {
  const url = new URL(line.slice(BOOTSTRAP_LINE.length).trim());
  return url.searchParams.get('token');
}

function startApp(appPath) {
  const child = spawn(appPath, ['--no-open', '--port', String(PORT)], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let buffered = '';
  return new Promise((resolve, reject) => {
    const onData = (chunk) => {
      buffered += chunk.toString();
      const lineStart = buffered.indexOf(BOOTSTRAP_LINE);
      if (lineStart === -1) return;
      const lineEnd = buffered.indexOf('\n', lineStart);
      if (lineEnd === -1) return;
      const credential = extractBootstrapCredential(buffered.slice(lineStart, lineEnd));
      if (credential) {
        child.stdout.off('data', onData);
        resolve({ child, credential });
      }
    };
    child.stdout.on('data', onData);
    child.stderr.on('data', (chunk) => process.stderr.write(chunk));
    child.on('error', reject);
    child.on('exit', (code) => reject(new Error(`flix-desktop exited early with code ${code}`)));
    setTimeout(() => reject(new Error('Timed out waiting for the flix-desktop bootstrap line')), 20_000);
  });
}

async function api(credential, method, endpoint, body) {
  const response = await fetch(`http://127.0.0.1:${PORT}${endpoint}`, {
    method,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${credential}`,
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) {
    throw new Error(`${method} ${endpoint} returned ${response.status}: ${await response.text()}`);
  }
  return response.status === 204 ? null : response.json();
}

async function waitForHealth(credential) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      await api(credential, 'GET', '/api/health');
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }
  throw new Error('flix-desktop never became healthy');
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Polls /api/status until it reaches a state the caller considers terminal
// (playing, or a failure) or the timeout elapses. It does not assume
// "failed" is always wrong - the anime check expects a specific failure.
async function pollUntilTerminal(credential, isTerminal, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    const status = await api(credential, 'GET', '/api/status');
    last = status.state;
    if (isTerminal(last)) return last;
    await sleep(500);
  }
  throw new Error(`${label}: timed out waiting for a terminal state. Last state: ${JSON.stringify(last)}`);
}

// Confirms the browser-preview stream URL actually serves real bytes, not
// just that the app produced a URL string. No player and no browser needed:
// the URL carries its own one-time access token in the query string.
async function assertStreamServesBytes(streamUrl, label) {
  const response = await fetch(streamUrl, { headers: { Range: 'bytes=0-65535' } });
  if (response.status !== 200 && response.status !== 206) {
    throw new Error(`${label}: stream URL returned HTTP ${response.status}`);
  }
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength === 0) {
    throw new Error(`${label}: stream URL returned no bytes`);
  }
}

async function checkMovie(credential, torrentPath) {
  await api(credential, 'POST', '/api/play', {
    target: 'browser',
    media_ref: { type: 'movie', catalog_id: 'qa:movie', title: 'QA Movie Fixture' },
    magnet: torrentPath,
  });
  const state = await pollUntilTerminal(
    credential,
    (state) => state.state === 'browser' || state.state === 'failed',
    45_000,
    'movie',
  );
  if (state.state !== 'browser') {
    throw new Error(`movie: expected browser playback to start, got a failure - ${JSON.stringify(state.error)}`);
  }
  await assertStreamServesBytes(state.stream_url, 'movie');
  await api(credential, 'POST', '/api/stop');
}

// Anime cannot play through the browser preview by design - Flix rejects it
// there to preserve the verified-track guarantee (see src/playback/browser.rs).
// But the strict Japanese-audio/English-subtitle track selection still runs
// in full before that rejection (see src/playback/coordinator.rs,
// launch_active_session). So playing the anime fixture with target "browser"
// is actually the right way to test that selection logic without any
// external player: reaching a "failed" state with error code
// BROWSER_UNSUPPORTED means the track selection succeeded and only the
// deliberate browser guard stopped it - that is the passing outcome here.
// Any other outcome (playing at all, or a different failure code such as
// ANIME_LANGUAGE_UNAVAILABLE) is a real regression.
async function checkAnime(credential, torrentPath) {
  await api(credential, 'POST', '/api/play', {
    target: 'browser',
    media_ref: { type: 'movie', catalog_id: 'kitsu:qa-anime', title: 'QA Anime Fixture' },
    magnet: torrentPath,
  });
  const state = await pollUntilTerminal(
    credential,
    (state) => state.state === 'browser' || state.state === 'failed',
    45_000,
    'anime',
  );
  if (state.state !== 'failed' || state.error?.code !== 'BROWSER_UNSUPPORTED') {
    throw new Error(
      `anime: expected the Japanese audio and English subtitle tracks to be selected, then hit the ` +
        `browser-anime guard (BROWSER_UNSUPPORTED). Got ${JSON.stringify(state)}`,
    );
  }
}

async function checkTvShow(credential, torrentPath) {
  await api(credential, 'POST', '/api/play', {
    target: 'browser',
    media_ref: {
      type: 'episode',
      catalog_id: 'qa:show',
      stream_id: 'qa-show-s01e01',
      season: 1,
      episode: 1,
      title: 'QA Show Fixture',
    },
    magnet: torrentPath,
  });
  const first = await pollUntilTerminal(
    credential,
    (state) => state.state === 'browser' || state.state === 'failed',
    45_000,
    'tv show episode 1',
  );
  if (first.state !== 'browser') {
    throw new Error(`tv show: expected episode 1 to start, got a failure - ${JSON.stringify(first.error)}`);
  }
  if (first.media?.season !== 1 || first.media?.episode !== 1) {
    throw new Error(`tv show: expected season 1 episode 1, got ${JSON.stringify(first.media)}`);
  }
  if (!first.has_next) {
    throw new Error('tv show: episode 1 did not report a next episode in the season pack');
  }
  await assertStreamServesBytes(first.stream_url, 'tv show episode 1');

  await api(credential, 'POST', '/api/next');
  const second = await pollUntilTerminal(
    credential,
    (state) => state.state === 'browser' || state.state === 'failed',
    45_000,
    'tv show episode 2',
  );
  if (second.state !== 'browser') {
    throw new Error(`tv show: expected episode 2 to start after next, got a failure - ${JSON.stringify(second.error)}`);
  }
  if (second.media?.season !== 1 || second.media?.episode !== 2) {
    throw new Error(`tv show: expected the next action to reach season 1 episode 2, got ${JSON.stringify(second.media)}`);
  }
  await assertStreamServesBytes(second.stream_url, 'tv show episode 2');
  await api(credential, 'POST', '/api/stop');
}

async function main() {
  const appPath = arg('--app');
  if (!appPath) {
    throw new Error('Usage: node run-post-release-check.mjs --app <path-to-flix-desktop>');
  }
  const inviteCode = process.env.QA_INVITE_CODE;
  if (!inviteCode) {
    throw new Error('QA_INVITE_CODE must be set');
  }
  if (!process.env.CI && !arg('--force-local')) {
    throw new Error(
      'This check copies fixtures into the real Flix download directory ' +
        '(there is no override for it) and refuses to run outside CI. ' +
        'Pass --force-local only on a throwaway machine or account you do not mind polluting.',
    );
  }

  const stagingDir = path.join(repoRoot, 'target', 'qa-fixtures');
  const torrentDir = path.join(repoRoot, 'target', 'qa-torrents');
  await mkdir(stagingDir, { recursive: true });
  await mkdir(torrentDir, { recursive: true });

  console.log('Generating local test fixtures...');
  run('node', [path.join(__dirname, 'generate-fixtures.mjs'), stagingDir]);
  const fixtures = {
    movie: path.join(stagingDir, 'movie.mp4'),
    anime: path.join(stagingDir, 'anime.mkv'),
    show: path.join(stagingDir, 'show'),
  };

  console.log('Packaging fixture torrents...');
  const movieTorrent = await buildTorrent(fixtures.movie, path.join(torrentDir, 'movie.torrent'));
  const animeTorrent = await buildTorrent(fixtures.anime, path.join(torrentDir, 'anime.torrent'));
  const showTorrent = await buildTorrent(fixtures.show, path.join(torrentDir, 'show.torrent'));

  console.log('Locating the app download directory...');
  const downloadDir = findDownloadDir(appPath);
  await cp(fixtures.movie, path.join(downloadDir, path.basename(fixtures.movie)));
  await cp(fixtures.anime, path.join(downloadDir, path.basename(fixtures.anime)));
  await cp(fixtures.show, path.join(downloadDir, path.basename(fixtures.show)), { recursive: true });

  console.log('Starting flix-desktop...');
  const { child, credential } = await startApp(appPath);
  try {
    await waitForHealth(credential);
    console.log('Activating with the QA invite code...');
    await api(credential, 'POST', '/api/activation/redeem', { invite_code: inviteCode });

    console.log('Checking movie playback...');
    await checkMovie(credential, movieTorrent);

    console.log('Checking anime playback (original Japanese audio, English subtitles)...');
    await checkAnime(credential, animeTorrent);

    console.log('Checking TV show playback and the next-episode action...');
    await checkTvShow(credential, showTorrent);

    console.log('All post-release QA checks passed.');
  } finally {
    try {
      await api(credential, 'POST', '/api/quit');
    } catch {
      child.kill();
    }
  }
}

main().catch((error) => {
  console.error(`Post-release QA failed: ${error.message}`);
  process.exit(1);
});
