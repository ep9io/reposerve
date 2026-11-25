# reposerve

Serve a repository's Markdown, source files and notebooks to a browser on the
local network.

`reposerve` is for reading a repository away from the desk: on a phone, a tablet
or a second machine. Start it with one command and open the address it prints.

## What it does

- Renders Markdown, including tables, task lists, footnotes and heading anchors.
- Lists the repository as a tree that keeps its paths, so a relative link between
  two documents resolves as it was written.
- Highlights source and fenced code on the server, so a page arrives ready to
  read.
- Reads Jupyter notebooks, read only: cells, stream output, images, errors and
  sandboxed HTML tables.
- Draws Mermaid diagrams.
- Offers a light, a dark and an automatic colour scheme.
- Filters the tree by file extension, with a choice of presets and your own.
- Reloads a page when a file changes.
- Requires a token, when you ask for one.

Pages are rendered on the server. The browser runs one script for diagrams, one
for live reload and one for the copy buttons.

## Install

```bash
cargo install reposerve
```

Run it from a checkout without installing:

```bash
cargo run --release -- --root . --port 8095
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

The default filter shows Markdown alone. Open **Files**, then **Change**, to add
source files, notebooks or images. The choice is a cookie, so each device keeps
its own.

### Options

| Option | Default | Meaning |
| --- | --- | --- |
| `-r, --root <DIR>` | `.` | The directory to serve |
| `-b, --bind <ADDRESS>` | `0.0.0.0` | The address to listen on |
| `-p, --port <PORT>` | `8095` | The port to listen on |
| `--title <TEXT>` | `files` | The name in the header and the browser title |
| `--token <VALUE>` | none | Require a token on the first visit |
| `-e, --extensions <LIST>` | `md` | The extensions shown on a first visit |
| `--include <LIST>` | the whole tree | Only serve these directory prefixes |
| `-c, --config <FILE>` | none | Read settings from a TOML file |
| `--no-live-reload` | off | Do not reload a page when a file changes |
| `--show-config` | off | Print the resolved settings and exit |

A value on the command line wins over the file, and the file wins over the
default.

### Configuration file

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
```

Every key is optional, and an unknown key is an error rather than a silent typo.

- `root` is the only directory the server reads. A path outside it is never
  served.
- `include` limits the site to the prefixes you name. An entry may not leave the
  root.
- `assets` names the extensions served even when the filter hides them, so an
  image a document embeds still loads.
- `presets` are the checkboxes in the filter form.

## What it serves

When the root is a Git working tree, the file list comes from
`git ls-files --cached --others --exclude-standard`. Ignored output and
machine-local files are then never served, and a new untracked file appears
without a restart.

A plain directory is walked instead. The walk skips `.git`, `target` and
`node_modules`.

## Live reload

A page reloads itself when a served file changes. The server watches the file
list once a second and tells each open page over a server-sent event, so no
browser script polls.

Turn it off with `--no-live-reload` or `live_reload = false`.

## Security

The server reads files. It does not write them, run them or evaluate them.

- Only the listed files are served. A request outside the list is a 404, and a
  symlink that points outside the root is refused.
- An HTML notebook output is placed in a sandboxed frame. The frame can be
  measured but cannot run script.
- `--token` asks each device for a token once. The first visit carries it as
  `?token=...`, and the server keeps it in a cookie for thirty days. The token is
  compared with an equality check, which suits a private network and does not
  suit the public internet.

> **Warning:** The server speaks plain HTTP and has no rate limits. Run it on a
> network you trust, and put it behind a proxy if you need more than that.

## Licence

MIT. The bundled Mermaid script is MIT as well; its notice is in
`assets/mermaid.LICENSE`.
