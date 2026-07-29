# Bundled games

Anything in this folder is copied into every installer and offered to the user by
**Settings → Install bundled games**, which copies it to a writable folder and adds
that folder to the library.

Right now the folder holds no games, and the app says so rather than showing a
button that does nothing. Drop a file in and it appears — no code change needed.

## What may go in here

Only games the project is entitled to redistribute:

- **Homebrew you wrote yourself.** Your own build, your own call.
- **Homebrew whose author permits redistribution.** Check the licence and record it
  below.

Commercial PSP games may **not** go in here, in any format, however they were
obtained. Dumping a UMD you own is generally fine for your own use; putting the
result in a public installer is distribution, which is a different thing entirely.
The app is built for people to point at their own dumps, and the ROM folder picker
exists for exactly that.

## What gets copied

Only recognised game containers: `.iso`, `.cso`, `.pbp`, `.elf` and `.prx`. Every
other file here — this readme, licences, cover art — ships in the bundle but is
deliberately left out of the user's ROM folder.

## Adding a game

1. Copy the file into this folder.
2. Add a row to the table below, so the licence claim travels with the binary.
3. Set `HAS_BUNDLED_GAME = true` in `site/app.js`. That reveals the download page's
   "a game is already included" panel, which must not appear while this folder is
   empty.
4. If the licence requires the text to accompany the binary, put it beside the
   game as `<name>.LICENSE.txt`.

Nothing else needs changing. The installer picks the file up from here, the release
workflow publishes it as a standalone asset, and the app lists it by name.

| File | Title | Author | Licence / permission |
| ---- | ----- | ------ | -------------------- |
| _none yet_ | | | |
