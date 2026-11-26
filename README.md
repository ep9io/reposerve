# reposerve

Serve a repository's Markdown, source files and notebooks to a browser on the
local network.

`reposerve` is for reading a repository away from the desk: on a phone, a tablet
or a second machine. Start it with one command. Open the address it prints.

![A notebook in the browser: a formula, a Python cell with its output, and a
diagram drawn in the page, beside the file tree and the notebook outline](https://raw.githubusercontent.com/ep9io/reposerve/main/docs/screenshot.png)

## What it does

- Renders Markdown: headings, tables, task lists, footnotes, callouts and
  diagrams.
- Highlights source files and fenced code on the server.
- Reads Jupyter notebooks, read only: cells, output, images, errors and tables,
  with an outline of the headings.
- Draws Mermaid diagrams and renders math with KaTeX.
- Offers a light, a dark and a system colour scheme.
- Finds a file by name, and finds text inside the open page.
- Moves to the file above or below in the same folder, from a bar that stays at
  the foot of the page.
- Shows an image in a page of its own: its size, the picture and a link to the
  original.
- Filters the tree by file extension.
- Reloads a page when a file changes, and keeps the reader's place.
- Hides the chrome in reading mode.
- Requires a token, when you ask for one.

Pages are rendered on the server. The browser loads one small script for the
controls. A diagram or math script loads only on a page that needs one.

## Install

```bash
cargo install reposerve --locked
```

`--locked` installs the dependency versions the release was built with.

To work from a checkout instead, run it with the configuration file:

```bash
cargo run --release -- -c config/reposerve.toml
```

## Use

Start the server in the directory to serve. It prints every address a reader can
open.

```bash
reposerve
```

```text
Serving 214 files from /home/you/project
  http://192.168.1.20:8095/
  http://127.0.0.1:8095/
```

A directory that holds a `README.md` opens on that file. The default filter shows
Markdown alone. Open the filter in the sidebar to add source files, notebooks or
images, then press Apply. The choice is a cookie, so each device keeps its own.

On a wide screen the appearance menu also offers a reading width: Standard,
Wide or Widest. The choice is a cookie, so each device keeps its own. On a
phone or tablet the column still fills the window.

### Options

| Option                    | Default        | Meaning                                      |
| ------------------------- | -------------- | -------------------------------------------- |
| `-r, --root <DIR>`        | `.`            | The directory to serve                       |
| `-b, --bind <ADDRESS>`    | `0.0.0.0`      | The address to listen on                     |
| `-p, --port <PORT>`       | `8095`         | The port to listen on                        |
| `--title <TEXT>`          | `files`        | The name in the header and the browser title |
| `--token <VALUE>`         | none           | Require a token on the first visit           |
| `-e, --extensions <LIST>` | `md`           | The file types shown on a first visit        |
| `--include <LIST>`        | the whole tree | Only serve these directory prefixes          |
| `--theme <MODE>`          | `auto`         | `auto`, `light` or `dark`                    |
| `--reading-width <REM>`   | `46`           | The width of the reading column              |
| `--no-math`               | off            | Do not render math                           |
| `--no-toc`                | off            | Do not show the table of contents            |
| `--hidden`                | off            | Show entries whose name starts with a dot    |
| `--open`                  | off            | Open a browser at start                      |
| `-c, --config <FILE>`     | none           | Read a configuration file                    |
| `--no-live-reload`        | off            | Do not reload a page automatically           |
| `--show-config`           | off            | Print the resolved configuration and exit    |

A value on the command line wins over the file, and the file wins over the
default.

### Configuration file

`reposerve` reads `reposerve.toml` from the root when the file exists. Name
another file with `--config`. The repository holds one at
`config/reposerve.toml`, which serves the showcase.

```toml
root = "."
bind = "0.0.0.0"
port = 8095
title = "handbook"
token = ""
live_reload = true
extensions = ["md", "txt"]
presets = ["md", "txt", "rs", "png"]
include = ["docs", "notes"]
assets = ["png", "jpg", "svg", "pdf"]
theme = "auto"
reading_width = 46
math = true
toc = true
hidden = false
open = false
```

Every key is optional. An unknown key is an error, not a silent typo.

- `root` is the only directory the server reads. A path outside it is never
  served.
- `include` limits the site to the prefixes you name. An entry may not leave the
  root.
- `assets` names the extensions served even when the filter hides them, so an
  image a document embeds still loads.
- `presets` adds ticks to the filter form. The form lists every type the root
  holds, so a preset only helps before a file of that type exists.

## What it serves

When the root is a Git working tree, the file list comes from
`git ls-files --cached --others --exclude-standard`. Ignored output and
machine-local files are then never served. A new untracked file appears without a
restart.

A plain directory is walked instead. The walk skips `.git`, `target` and
`node_modules`.

An entry whose name starts with a dot is left out, so `.gitignore`, `.github/`
and `.env` never appear. Add `--hidden` or `hidden = true` to show them, except
`.git`, `target` and `node_modules`.

## Live reload

The server watches the root. When a file changes, it tells each open page which
path changed. A page reloads only when its own file changed, and it returns to
the same place on the page. The new page shows a short notice, which fades after
a few seconds or closes on Dismiss.

An editor's swap, backup and probe files do not count as changes: typing in `vi`
does nothing until the file is written. Hidden entries and paths outside the
served set are ignored too.

A change elsewhere shows a notice with a Reload button, so the reader can refresh
the tree when convenient.

Turn automatic reload off with `--no-live-reload` or `live_reload = false`. The
server still watches: the notice stays, and the button reloads only when the
reader asks.

## Security

The server reads files. It does not write them, run them or evaluate them.

- Only the listed files are served. A request outside the list is a 404. A
  symlink that points outside the root is refused.
- Every file of an enabled type is readable by anyone who can reach the port.
  Enable a type only for a repository you are willing to share.
- A notebook HTML output is cleaned before it reaches the page. Scripts, event
  handlers and remote resources are removed.
- Every page carries a content security policy that allows this server alone.
- `--token` asks each device for a token once. The form keeps the token out of
  the address bar, and the server keeps it in a cookie for thirty days.

> **Warning:** The server speaks plain HTTP and has no rate limit. Run it on a
> network you trust. Put it behind a proxy if you need more than that.

Read `SECURITY.md` for the threat model and the reporting route.

## Licence

MIT. The bundled Mermaid and KaTeX scripts are MIT as well. Their notices sit
beside them in `assets/`.
