# flix

Flix is a terminal torrent streamer. It serves torrent files through a local HTTP range server. It starts mpv or VLC while the download continues.

Flix can search Stremio-compatible catalogs and torrent stream add-ons. You can also supply a magnet URL, an HTTP or HTTPS torrent URL, or a local `.torrent` file.

## Components

Flix has three parts:

- The Rust CLI and TUI in `src/`.
- The desktop web application in `web/`. Svelte and TypeScript supply the interface. The `flix-desktop` binary serves it on `127.0.0.1`. Search, episode selection, stream picking, and playback all work through the browser.
- The optional Cloudflare Workers gateway in `gateway/`. It supplies shared invite, device, and subtitle services. The maintainer runs one instance; you can deploy your own.

## Requirements

- A current stable Rust toolchain for installation from source.
- A network connection for torrent peers and optional metadata.
- mpv or VLC for playback.
- A terminal that supports the alternate screen for the TUI.
- Node.js 20 or later, but only for the desktop web application.

mpv supplies live playback position through IPC on macOS and Linux. VLC playback works without this position. Windows playback works, but live mpv position is not available in this release.

## Install

Install Flix from this source directory:

```sh
cargo install --path .
```

Flix can show and run a supported package-manager command for mpv:

```sh
flix install-player
```

Flix prints the exact command first. It requires an explicit `yes` response before it starts the package manager.

You can also install mpv manually from <https://mpv.io/installation/>. Flix detects `mpv` and `vlc` on `PATH`. On macOS, it also checks the standard application paths.

## Desktop application

The desktop application needs the `web-ui` feature. Build the web interface first, because the `flix-desktop` binary embeds `web/dist` at compile time:

```sh
npm --prefix web install
npm --prefix web run build
touch web/dist/.gitkeep
cargo build --release --features web-ui --bin flix-desktop
./target/release/flix-desktop
```

The `web/dist` directory holds build output, so Git ignores its contents. A new clone has an empty `web/dist` directory. The Rust build succeeds with the empty directory, but the desktop application serves no page until you run the web build.

The web app opens automatically in your default browser. It can search Stremio catalogs, browse episodes, pick torrent streams, and launch your local media player. Poster images, quality badges, and seed counts help you choose a stream.

Search results appear as each catalog responds. You can open results while other catalogs load. A new search cancels older browser requests.
Flix caches successful catalog and metadata responses in memory for five minutes. The cache holds at most 64 responses and 8 MiB.
If Anime Kitsu fails or takes more than six seconds, Flix checks matching general catalog results for anime metadata.
This fallback checks at most 16 titles. Provider failures remain visible in the search view.

Use these options:

- `--no-open`: Start the server, but do not open the browser.
- `--check`: Run a system check and exit.
- `--port <PORT>`: Bind the local web server to a fixed port. Omit it to use any free port. A fixed port lets a second instance run beside the first.

The desktop server binds only to `127.0.0.1`. Every API request requires a token that the process creates in memory.

Merging to `main` runs continuous integration checks. A successful run on `main` starts the release workflow. A manual release trigger also remains available.

### Release 0.4.0

This release adds an updater module, packaged application icons, and improved player focus behavior.
It also updates the release automation pipeline.

The unsigned installers are `Flix-macOS-universal.dmg` and `Flix-Windows-x64-Setup.exe`.
The macOS installer supports Apple silicon and Intel Macs. The Windows installer targets x64 systems.
The macOS package remains unsigned.
Installer packages contain only the application executable and embedded web assets.
Activation keys and application data remain outside the installation directory.
Existing activation, profiles, lists, progress, downloads, and cached subtitles persist across updates.

### Desktop updates

The desktop web application checks for new releases on GitHub.
When an update is available, you can inspect the version notes and install the update.

- On Windows, Flix verifies the installer checksum and starts a background helper process.
The helper waits for Flix to exit and runs the installer silently.
- On macOS, Flix verifies the disk image checksum and opens the DMG file.
Drag `Flix` to the Applications folder to replace the older version.

The release workflow publishes verified packages with `SHA256SUMS.txt`.
Users can download installers from the [GitHub releases page](https://github.com/BabalolaBrainiac/flix/releases).

### Install a packaged build on macOS

The macOS DMG is not signed with an Apple Developer ID. macOS can block its first launch because it cannot verify the publisher.

Open it once with either method:

1. In Finder, right-click (or Control-click) `Flix`, choose **Open**, then **Open** again in the dialog. macOS remembers the choice for later launches.
2. Or remove the download quarantine flag in a terminal:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Flix.app
   ```

A locally built binary does not show this message, because only files downloaded through a browser get the quarantine flag.

For an illustrated, non-technical walkthrough, see [docs/instructions/macos.md](docs/instructions/macos.md).

### Install a packaged build on Windows

Run `Flix-Windows-x64-Setup.exe` and follow the installer steps. Windows can show an unknown publisher warning because the installer is unsigned.
Open Flix from the Start menu. Enter your invite code when the activation page opens.
Use VLC or mpv for anime and formats that the browser preview does not support.

For an illustrated, non-technical walkthrough, see [docs/instructions/windows.md](docs/instructions/windows.md).

## Gateway

The Cloudflare Workers gateway is in `gateway/`. The maintainer runs an instance that needs an invite code. Flix works in local mode without a gateway, so deploy your own from this directory if you want shared subtitle services.

```sh
npm --prefix gateway install
npm --prefix gateway run type-check
npm --prefix gateway test
```

The gateway reads `OPENSUBTITLES_API_KEY`, `OPENSUBTITLES_USER_AGENT`, `OPENSUBTITLES_USERNAME`, and `OPENSUBTITLES_PASSWORD` from the Worker environment. Add each value as a Wrangler secret. Do not put a value in `wrangler.toml`.

## Tests

```sh
cargo fmt --check
cargo clippy --release --all-targets --features web-ui -- -D warnings
cargo test --features web-ui --all-targets
npm --prefix web test
npm --prefix gateway test
```

Cargo ignores one subtitle test. That test needs valid OpenSubtitles credentials and uses one account download.

## Use

Add a torrent and save it in the library:

```sh
flix add <source>
```

Start the local stream server:

```sh
flix serve <source>
```

Load the torrent, select an episode or video, and start it in a detected player:

```sh
flix play <source>
```

If the torrent contains a season pack, Flix lists the supported video files in season and episode order. Enter the number for the episode that you want. Flix prepares the start and end of the file before it launches the player.

Flix selects only the chosen episode for download. It changes this selection when you choose another episode. This prevents a season pack from dividing bandwidth across all episodes.

`flix play` stores torrent pieces in a temporary playback cache. Flix removes this cache after a normal exit. Use the TUI or `flix serve` when you want to keep downloaded media.

During playback, focus the terminal and use these controls:

- Press `n` to prepare and play the next episode.
- Press `s` to change to another torrent source of the same title.
- Press `q` to stop playback and exit.

Flix keeps the current episode open while it prepares the requested next episode. It hides player diagnostics during normal playback. Set `FLIX_PLAYER_LOGS=1` before startup when you need VLC or mpv diagnostics.

After the player exits, use these actions:

- Press `Enter` or `n` to play the next episode.
- Press `s` to change to another torrent source of the same title.
- Press `l` or `b` to show the episode list.
- Press `r` to replay the current episode.
- Press `q` to stop the torrent session and exit.

Flix keeps the source list from the search. The `s` action shows that list, so a
source that does not play does not make you repeat the search. Flix also prints
a hint when the player closes less than 25 seconds after start, because a source
that fails usually closes the player at once.

If the magnet link is on the macOS clipboard, run:

```sh
flix play "$(pbpaste)"
```

Open the terminal interface:

```sh
flix tui
```

## Search

Search movies, series, and anime:

```sh
flix search "Lioness"
```

Search only the anime catalog:

```sh
flix search --anime "Frieren"
```

At each search prompt, press `b` to go back one step and `q` to stop the search. The back action returns from the source list to the episode list. It returns from the episode list to the season list. It returns from the season list to the title list. Flix keeps the results it already loaded, so a back step does not repeat a request.

Flix lists titles, seasons, episodes, and torrent releases. It ranks native 4K releases first. It ranks 1080p releases next. It ranks seed count within the same quality. It does not rank an identified AI upscale as native 4K.

Press `Enter` at the torrent prompt to use the displayed default. If the preferred 4K torrent cannot start, Flix tries up to two other native 4K torrents. It then tries the best 1080p torrent. Generated magnets include public tracker hints, and Flix keeps its DHT routing cache to improve later startup times. Select a release, then choose play, download, or magnet output.

For a series search, Flix keeps the episode list after playback starts. The next action uses the next pack file or searches the next catalog episode. You do not need to repeat the title search.

In the web app, select an episode before Flix requests its sources. Switching seasons clears the selection. Movies show their sources directly.

Use a direct action when you do not want the final action prompt:

```sh
flix search --download "Lioness"
flix search --anime --magnet "Frieren"
```

Flix uses the official Cinemeta catalog for movies and series. It uses Anime Kitsu for anime metadata. It uses Torrentio for torrent stream results. Anime Kitsu and Torrentio are community services. Their availability and results can change.

You can replace the default HTTPS add-on bases:

```sh
export FLIX_CINEMETA_URL=https://example.com/
export FLIX_ANIME_KITSU_URL=https://example.com/
export FLIX_TORRENT_STREAM_URL=https://example.com/
```

`<source>` accepts these values:

- A magnet URL with a BitTorrent info hash.
- An `http://` or `https://` torrent URL.
- A local path with the `.torrent` extension.

## TUI keys

- `1`: Open Home.
- `2`: Open Add.
- `3`: Open Detail.
- `4`: Open Downloads.
- `Tab`: Open the next tab.
- `Up` and `Down`: Change the selected item.
- `Enter`: Open, add, or play the selected item.
- `p`: Play the selected file on Detail.
- `n`: Play the next episode on Detail.
- `s`: Change the Home sort.
- `Esc`: Clear Add input and return Home.
- `q`: Exit outside the Add tab.

The Add tab treats all typed characters as input. This includes `q` and the digits `1` to `4`.

## Metadata

Set `TMDB_API_KEY` to enable TMDB movie metadata:

```sh
export TMDB_API_KEY=<your-key>
```

Do not place the key in the repository. Flix works without this key. Missing scores appear as `-`.

Letterboxd metadata is optional. Flix reads public JSON-LD data when it adds a new library entry. A Letterboxd error does not stop the torrent.

## Browser player preview

Select a source, then choose **Browser preview** under **Play with**. MP4, M4V, and WebM files can play inside Flix without VLC.

The browser must support the video and audio codecs in the file. Flix sends the original encoded bytes without conversion. Browser and display capabilities still affect playback.

Use the video controls for seeking, volume, and fullscreen. The buttons below the video provide pause, resume, and ten-second jumps. Next episode appears when available.

Flix reports playback from browser events. If the browser blocks automatic playback, select **Play**. Reloading the page restores the latest reported position.

The player stays open when you change Flix tabs. Stop, playback end, and media errors release the session. Three minutes without browser updates also trigger cleanup.

Flix converts available external English SRT subtitles to WebVTT in memory. Stream URLs apply only to the selected file. Changing episodes invalidates old URLs.

MKV playback and verified anime track selection still require an external player. The browser preview rejects anime playback to preserve the language requirement.

Styled subtitles, wider codec support, and timed intro skipping remain in [issue #7](https://github.com/BabalolaBrainiac/flix/issues/7). This preview does not add intro skip buttons.

### Browser playback check

Install FFmpeg and the web development dependencies. Install the Chromium test browser once:

```sh
npm --prefix web ci
cd web
npx playwright install chromium
npm run test:browser
```

The check builds the web app and generates a short sample video. It starts a local Rust test host and checks decoded frames, subtitles, controls, and cleanup.

Set `FLIX_TEST_CHROME` to use an existing Chromium executable. The check removes its temporary media and control file when it finishes.

## Recommendations and manga

Open **Discover** for recommendations. Open **Reader** to search MangaDex, select a chapter, and save your reading position.

The CLI supports these actions:

```sh
flix recommend "space adventure" --movie --limit 10
flix recommend "fantasy" --anime --json
flix manga search "title" --lang en
flix manga read <manga-id> --lang en
flix manga read <manga-id> --export-cbz chapter.cbz
```

CBZ exports store chapter images in a ZIP archive. Reader queries currently inspect up to 500 chapter records per title.

## Anime audio and subtitles

Anime search removes sources that explicitly offer only dubbed audio. Dual audio sources remain eligible for a track check.

The web app also checks general catalog metadata before selecting sources. Japanese animation receives the anime language rule, including IMDb results.
Flix keeps the original provider and episode identifiers for these results.
Debug reports include search duration and verified track indices. They exclude search text, media names, and media URLs.

The CLI and web app check MKV track labels before anime playback. Flix selects a track marked as original. Otherwise, it selects Japanese audio.

Flix selects labeled English subtitles. It excludes embedded tracks marked as forced, signs, or songs. An external English subtitle can satisfy this requirement.

Flix rejects unknown audio and missing English subtitles. It does not use the release default to bypass this check. Other file formats cannot pass this strict check yet.

These checks use file metadata. Incorrect language labels can still give incorrect results. Pasted magnets without anime catalog context use the standard playback settings.

## Audio language

Other playback prefers English audio and English subtitles. Set `FLIX_AUDIO_LANGUAGE` to change the audio preference outside anime search:

```sh
export FLIX_AUDIO_LANGUAGE=jpn,ja,japanese
```

## Debug reports and cleanup

Open **Diagnostics** to export a debug report. Use **Open an issue** to report a problem or request a feature.

Reports contain the app version, operating system, session ID, playback stages, elapsed times, and error codes. They also record player exits, exit codes, signals, and process duration. Reports exclude titles, paths, source links, and credentials.

Flix checks for immediate player exits before it shows the player as open. The status does not confirm video decoding. Later player errors remain visible until you dismiss them.

Flix keeps up to 256 events per local session report. Startup cleanup limits older reports to 2 MiB and seven days. Flix does not upload reports.

Playback uses a temporary directory. Stop, normal player exit, and application shutdown release the session. Startup cleanup removes abandoned playback directories and keeps locked active directories.

Reader page cleanup limits the cache to 128 MiB. Startup cleanup limits subtitle files to 32 MiB. Reading progress and saved downloads remain separate.

A selected video can still occupy its file size during streaming. Flix removes its temporary data after playback. Saved downloads remain until you remove them.

### Local release checks

Build the web interface and desktop binary with the commands above. Close your old Flix instance before you start the new binary.

```sh
./target/release/flix-desktop --check
./target/release/flix-desktop
```

The system check confirms core setup and player detection. It does not test live video playback.

1. Open a series result. Confirm that sources remain hidden until you select an episode.
2. Switch seasons. Confirm that the old selection and its sources disappear.
3. Play an anime episode. Confirm original audio and English subtitles in the player.
4. Use Stop during loading, buffering, and playback. Start another episode after each stop.
5. Close the player. Confirm that Flix releases the playback session.
6. Leave the browser tab during playback. Return and check that the status updates.
7. Start a second Flix instance. Confirm that Stop affects only its own player.
8. Export a debug report from Diagnostics. Check the player exit details after a failed launch.

For a local VLC process check, supply a sample video that lasts more than four seconds:

```sh
cargo run --example player_probe -- /path/to/sample.mkv
```

This check opens VLC briefly and stops its process. It does not confirm video or subtitle quality.

## English subtitles

Flix searches OpenSubtitles for the selected episode. The CLI lists up to 20 English subtitle releases before download. Select one result, several results as `1,2`, `a` for the top results, or `0` to play without an external subtitle. Flix downloads at most 3 subtitles.

Flix downloads every selected subtitle before it starts the player. It then gives each file to the player as a selectable subtitle track. mpv receives one `--sub-file` option for each track. VLC receives the first track through `--sub-file` and the other tracks through `--input-slave`. The VLC subtitle menu therefore lists every downloaded subtitle. Other playback continues if subtitle search or download fails. Anime playback requires an English subtitle.

Flix reuses subtitles that it downloaded before for the same torrent video. In that case it gives all cached files to the player and does not ask again.

For anime started through `flix search --anime`, Flix first downloads an English subtitle from the Anime Kitsu Stremio subtitle resource. This path does not use the OpenSubtitles account login. Flix uses the OpenSubtitles API as the fallback. Pasted magnets have no Kitsu identifier, so they use the OpenSubtitles path only.

Set these environment variables before you start Flix:

```sh
export OPENSUBTITLES_API_KEY=<your-api-key>
export OPENSUBTITLES_USER_AGENT=<your-user-agent>
export OPENSUBTITLES_USERNAME=<your-username>
export OPENSUBTITLES_PASSWORD=<your-password>
```

Do not put these values in the repository. Keep them in a local file that Git ignores, then load that file into the current shell:

```sh
set -a
source ./.env
set +a
```

Flix stores completed subtitle files in the `subtitles` data subdirectory. The file name holds the torrent info hash, the video file index, and the subtitle identifier. It removes an incomplete subtitle file after a failed write.

If login returns `401 Unauthorized`, the API key can still return search results, but OpenSubtitles will not issue a download link. Confirm the username and password for the OpenSubtitles.com account. Then update the environment and start a new Flix process. Flix stops login attempts for the current process after one 401 response.

## Data and security

Flix uses the platform application-data directory. It stores downloads in the `downloads` subdirectory. It stores the library in `library.json`.

The stream server binds only to `127.0.0.1`. It uses an in-memory token on every request. It also limits the request rate. Flix does not start a background service.

The library uses a temporary file and an atomic rename. This prevents a partial library write from replacing the last valid file.

Flix stops the local server, player, IPC monitor, torrent tasks, and pending work when the process exits. The `play` command removes its temporary media cache after a normal exit. The TUI and `serve` command retain downloaded media. Flix retains completed subtitles.

## Current limits

- Flix supports one active player from the TUI.
- Stremio search depends on external catalog and torrent stream add-ons.
- Letterboxd is an optional public-page integration. Its page format can change.
- The first release build can take several minutes because librqbit has a large dependency tree.

## License

Flix is released under the [MIT License](LICENSE).

The default gateway URL `https://gateway.babalola.dev` points at the maintainer's
private subtitle gateway, which needs an invite code. If you build Flix yourself,
deploy your own gateway from the `gateway/` directory, or set `FLIX_GATEWAY_URL`
to your own instance. The torrent, playback, search, and TUI features work
without any gateway.
