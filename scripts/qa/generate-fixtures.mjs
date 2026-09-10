// Builds the local test videos for the post-release QA check. Every file is
// short, synthetic, and generated with ffmpeg. Nothing here comes from a
// live catalog or a public tracker, so the check has no external content
// dependency and no copyright question.
//
// Usage: node generate-fixtures.mjs <output-directory>

import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ['ignore', 'inherit', 'inherit'] });
    child.on('error', reject);
    child.on('exit', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with code ${code}`));
    });
  });
}

const FFMPEG_BASE = ['-hide_banner', '-loglevel', 'error', '-y'];

async function buildMovie(outDir) {
  const target = path.join(outDir, 'movie.mp4');
  await run('ffmpeg', [
    ...FFMPEG_BASE,
    '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=24',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000',
    '-t', '6',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-movflags', '+faststart',
    target,
  ]);
  return target;
}

async function buildAnime(outDir) {
  const subtitlePath = path.join(outDir, 'anime.srt');
  await writeFile(
    subtitlePath,
    '1\n00:00:00,000 --> 00:00:06,000\nQA fixture dialogue line.\n',
    'utf8',
  );
  const target = path.join(outDir, 'anime.mkv');
  await run('ffmpeg', [
    ...FFMPEG_BASE,
    '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=24',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000',
    '-f', 'lavfi', '-i', 'sine=frequency=660:sample_rate=48000',
    '-i', subtitlePath,
    '-t', '6',
    '-map', '0:v', '-map', '1:a', '-map', '2:a', '-map', '3:s',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-c:s', 'srt',
    '-metadata:s:a:0', 'language=eng',
    '-metadata:s:a:0', 'title=English dub',
    '-metadata:s:a:1', 'language=jpn',
    '-metadata:s:a:1', 'title=Japanese original',
    '-metadata:s:s:0', 'language=eng',
    '-metadata:s:s:0', 'title=Full dialogue',
    '-disposition:s:0', 'default',
    target,
  ]);
  return target;
}

async function buildShow(outDir) {
  const showDir = path.join(outDir, 'show');
  await mkdir(showDir, { recursive: true });
  const episodes = [];
  for (const [season, episode] of [[1, 1], [1, 2]]) {
    const target = path.join(showDir, `Flix.QA.Show.S${String(season).padStart(2, '0')}E${String(episode).padStart(2, '0')}.mp4`);
    await run('ffmpeg', [
      ...FFMPEG_BASE,
      '-f', 'lavfi', '-i', `testsrc2=size=640x360:rate=24`,
      '-f', 'lavfi', '-i', `sine=frequency=${330 + episode * 40}:sample_rate=48000`,
      '-t', '4',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-movflags', '+faststart',
      target,
    ]);
    episodes.push(target);
  }
  return { showDir, episodes };
}

async function main() {
  const outDir = process.argv[2];
  if (!outDir) {
    console.error('Usage: node generate-fixtures.mjs <output-directory>');
    process.exit(1);
  }
  await mkdir(outDir, { recursive: true });
  const movie = await buildMovie(outDir);
  const anime = await buildAnime(outDir);
  const show = await buildShow(outDir);
  console.log(JSON.stringify({ movie, anime, show: show.showDir, episodes: show.episodes }, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
