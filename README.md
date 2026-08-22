# flix

Flix is a terminal torrent streamer. It serves torrent files through a local HTTP range server. It starts mpv or VLC while the download continues.

Flix can search Stremio-compatible catalogs and torrent stream add-ons. You can also supply a magnet URL, an HTTP or HTTPS torrent URL, or a local `.torrent` file.

## Components

Flix has three parts:

- The Rust CLI and TUI in `src/`.
- The desktop web application in `web/`. Svelte and TypeScript supply the interface. The `flix-desktop` binary serves it on `127.0.0.1`. Search, episode selection, stream picking, and playback all work through the browser.
- The optional Cloudflare Workers gateway in `gateway/`. It supplies shared invite, device, and subtitle services. The gateway is not deployed.

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
cargo build --release --features web-ui --bin flix-desktop
./target/release/flix-desktop
```

The `web/dist` directory holds build output, so Git ignores its contents. A new clone has an empty `web/dist` directory. The Rust build succeeds with the empty directory, but the desktop application serves no page until you run the web build.

The web app opens automatically in your default browser. It can search Stremio catalogs, browse episodes, pick torrent streams, and launch your local media player. Poster images, quality badges, and seed counts help you choose a stream.

Use these options:

- `--no-open`: Start the server, but do not open the browser.
- `--check`: Run a system check and exit.
- `--port <PORT>`: Bind the local web server to a fixed port. Omit it to use any free port. A fixed port lets a second instance run beside the first.

The desktop server binds only to `127.0.0.1`. Every API request requires a token that the process creates in memory.

### Install a packaged build on macOS

The macOS DMG from the release page is not signed with an Apple Developer ID, so macOS blocks the first launch with a message like "Apple could not verify Flix is free of malware". This is expected for an unsigned app. The application is safe to run; macOS only refuses to launch it automatically.

Open it once with either method:

1. In Finder, right-click (or Control-click) `Flix`, choose **Open**, then **Open** again in the dialog. macOS remembers the choice for later launches.
2. Or remove the download quarantine flag in a terminal:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Flix.app
   ```

A locally built binary does not show this message, because only files downloaded through a browser get the quarantine flag.

## Gateway

The Cloudflare Workers gateway is in `gateway/`. It is not deployed, and no gateway URL exists. Flix works in local mode without it.

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

## Audio language

Many torrent releases hold more than one audio track and mark a track other than
English as the default. Flix therefore tells the player which language to prefer.
It passes `--alang` and `--slang` to mpv, and `--audio-language` and
`--sub-language` to VLC. The default preference is `eng,en,english`.

The player still uses the release default when no track declares one of these
languages. Change the preference with a comma separated list, most preferred
first:

```sh
export FLIX_AUDIO_LANGUAGE=jpn,ja,japanese
```

## English subtitles

Flix searches OpenSubtitles for the selected episode. The CLI lists up to 20 English subtitle releases before download. Select one result, several results as `1,2`, `a` for the top results, or `0` to play without an external subtitle. Flix downloads at most 3 subtitles.

Flix downloads every selected subtitle before it starts the player. It then gives each file to the player as a selectable subtitle track. mpv receives one `--sub-file` option for each track. VLC receives the first track through `--sub-file` and the other tracks through `--input-slave`. The VLC subtitle menu therefore lists every downloaded subtitle. Playback continues if subtitle search or download fails.

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
