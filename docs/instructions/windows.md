# Flix for Windows

This guide is for a first-time user. It does not need any technical
knowledge. It covers install, first launch, activation, and playback.

## What Flix does

Flix finds movies, TV shows, and anime. It plays them through your web
browser or through VLC. You need an invite code to activate your copy.

## Step 1: Install VLC

Flix uses VLC to play some video files, such as anime and MKV files.
Install VLC before you install Flix.

1. Go to <https://www.videolan.org/vlc/>.
2. Click **Download VLC**.
3. Run the downloaded installer and follow its steps.

## Step 2: Download Flix

1. Go to the [Flix releases page](https://github.com/BabalolaBrainiac/flix/releases).
2. Find the newest release at the top of the page.
3. Click **Flix-Windows-x64-Setup.exe** to download it.

## Step 3: Open Flix for the first time

Flix does not have a Windows publisher signature yet. Windows Defender
SmartScreen blocks an unsigned installer on its first run. This is
normal. Use the steps below to run the installer.

1. Open the downloaded **Flix-Windows-x64-Setup.exe** file.
2. A blue window says **Windows protected your PC**.
3. Click **More info** in that window.
4. Click **Run anyway**.
5. The Flix setup wizard opens.

## Step 4: Complete the setup wizard

1. Follow each screen in the setup wizard.
2. Keep the default install location, unless you need a different one.
3. Click **Install**, then click **Finish** when setup completes.
4. Flix adds a shortcut to the Start menu.

## Step 5: Activate your device

1. Open **Flix** from the Start menu.
2. Flix opens a page in your web browser.
3. The page asks for an invite code.
4. Type the invite code that you received, then click **Activate device**.
5. Flix registers your device with the invite code. One invite code
   activates a fixed number of devices. Ask the code owner for a new
   code if activation fails.

## Step 6: Find something to watch

1. Type a title into the search box, then press **Enter**.
2. Flix shows matching movies, TV shows, and anime.
3. Select a title, then select a season and episode for a TV show or
   anime.
4. Select a stream. Flix shows the video quality and a seed count for
   each stream. A higher seed count usually starts playback faster.
5. Click **Play**. Flix opens the video in your browser or in VLC.

Anime plays with its original Japanese audio and English subtitles.
Flix checks each anime file for these tracks before it starts playback.

## Step 7: Player controls

- Use the on-screen controls to pause, resume, seek, and change volume.
- Click **Next episode** to play the next episode in a series.
- Anime and MKV files open in VLC. Use the VLC window controls for
  these files.
- Live playback position update in VLC is not available on Windows in
  this release. Use the browser progress bar for MP4, M4V, and WebM
  files instead.

## Troubleshooting

### No audio, or the wrong subtitle language

- Open the VLC **Audio** menu to change the audio track.
- Open the VLC **Subtitle** menu to change the subtitle track.
- In the browser player, use the subtitle control below the video.

### VLC is missing

Flix shows an install prompt when it cannot find VLC. Follow Step 1
above, or download VLC directly from <https://www.videolan.org/vlc/>.

### Report a problem

1. Open **Diagnostics** in the Flix web page.
2. Click **Export report**. This creates a file with app version,
   operating system, and playback details. It does not include titles,
   file paths, or your invite code.
3. Open an issue on the [Flix issues page](https://github.com/BabalolaBrainiac/flix/issues)
   and attach the report.
