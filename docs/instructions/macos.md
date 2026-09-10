# Flix for macOS

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
3. Open the downloaded file and drag **VLC** to **Applications**.

## Step 2: Download Flix

1. Go to the [Flix releases page](https://github.com/BabalolaBrainiac/flix/releases).
2. Find the newest release at the top of the page.
3. Click **Flix-macOS-universal.dmg** to download it.

## Step 3: Install Flix

1. Open the downloaded `Flix-macOS-universal.dmg` file.
2. A window opens with the Flix icon and an Applications folder icon.
3. Drag the **Flix** icon onto the **Applications** icon.
4. Wait for the copy to finish, then close the window.

## Step 4: Open Flix for the first time

Flix does not have an Apple developer signature yet. macOS blocks an
unsigned app on its first launch. This is normal. Use one of the two
methods below to open Flix.

### Method A: Right-click to open

1. Open **Applications** in Finder.
2. Right-click (or Control-click) the **Flix** icon.
3. Choose **Open** from the menu.
4. A dialog says macOS cannot check the app for malicious software.
5. Click **Open** in that dialog.
6. macOS remembers this choice. Later launches do not show the dialog.

If macOS still blocks the app, use this alternative method:

1. Open **System Settings**.
2. Click **Privacy & Security**.
3. Scroll to the **Security** section.
4. Find the message about Flix, then click **Open Anyway**.
5. Confirm **Open** in the next dialog.

### Method B: Terminal command

Use this method only if Method A does not work, or if you prefer a
terminal command. A terminal is an app that runs typed commands.

1. Open **Terminal**. Find it in **Applications** > **Utilities**.
2. Type this command, then press **Return**:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Flix.app
   ```

3. Open **Flix** from **Applications**. It now opens without a warning.

## Step 5: Activate your device

1. Flix opens a page in your web browser.
2. The page asks for an invite code.
3. Type the invite code that you received, then click **Activate device**.
4. Flix registers your device with the invite code. One invite code
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
