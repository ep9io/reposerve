# Changelog

Every change that a reader can see is listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 26 November 2026

### Fixed

- The configured reading width now reaches the page. The stylesheet default
  used to win over the value the server wrote into the markup.
- Live reload ignores an editor's swap, backup and probe files, hidden entries
  and any path that is not served. Typing in `vi` no longer raises the change
  notice before a save.
- Ctrl-C stops the server. The file watcher now ends on a stop request, so the
  runtime no longer waits for a file change. A browser holding the live-reload
  stream open can delay the exit by at most two seconds.

### Added

- A reading width control in the appearance menu: Standard, Wide and Widest.
  It changes the desktop column and is remembered per device in a cookie. The
  table of contents and the file tree stay beside the text rather than at the
  edges of a large screen.
- An image opens as a page of its own: its name, its size, the picture and a
  link to the original. A document that embeds an image keeps loading it.

### Changed

- The file tree and the text keep the same position when the reader moves
  between a document and a file without a table of contents. The contents
  column stays reserved, and a short page keeps the scrollbar's width.
- A source file, a binary file and an image no longer repeat their path as a
  heading under the breadcrumb.
- A stop request prints `Stopping the server.` before the short grace period
  for open pages.
- After a live reload the page shows a short notice with Dismiss, which fades
  after five seconds. A change to another file keeps a notice with Reload.
- With automatic reload off, the server still watches, the notice stays, and
  Reload waits for the reader.
- The file-type filter applies when Apply is pressed. Ticking a box no longer
  reloads the page or closes the filter on a phone.
- The filter offers a Documents only shortcut, which selects every document
  type the repository holds. It replaces Markdown only.
- The page shown when the filter hides everything offers Show Documents, which
  selects every document type, in place of Show Markdown.
- The breadcrumb and the Source button stay under the header while the page
  scrolls, so they are always in reach.
- Previous and next sit in a bar at the foot of the page, on every file view.
  On a narrow screen the table of contents sits between them, and on a wide
  screen a link back to the top does.

### Removed

- The "All files" link at the foot of a page. The breadcrumb and the brand both
  reach the root.

## [0.1.0] - 25 November 2026

The first release. It serves one directory to a browser on the local network.

### Added

- Server-rendered Markdown: headings, tables, task lists, footnotes, callouts
  and links from each heading to its own section.
- Syntax highlighting for source files and fenced code, with a palette for each
  theme. The highlighter needs no C compiler.
- Read-only Jupyter notebooks: code, output, images, tables, tracebacks, and an
  outline of the Markdown headings.
- Mermaid diagrams and KaTeX math, each loaded only on a page that needs it.
- Three colour schemes: `auto`, `light` and `dark`, remembered per device.
- Reading mode, with a floating control that leaves it and changes the scheme.
  The control works without a script.
- A file palette on `Ctrl`/`Cmd`+`K`, and a find bar on `Ctrl`/`Cmd`+`F`.
- A tree that opens the path to the current page, a filter by file type, and
  previous and next links inside a folder. The filter groups the types into
  documents, code and other, and shows how many files each type has.
- Live reload: only the page whose file changed reloads, and it keeps its place.
- Compressed responses, an entity tag for each file, an in-memory page cache and
  fingerprinted asset URLs.
- `reposerve.toml`, read from the root or named with `--config`.
- A shared token for the first visit. The form keeps the token out of the
  address bar.

### Security

- Every response carries a content security policy, `nosniff`, a referrer policy
  and a frame policy.
- Notebook HTML output is cleaned before it reaches the page.
- The token is compared in constant time and held in a cookie for thirty days.
- A request outside the file list is a 404, and a symlink that leaves the root
  is refused.

### Notes

- The server speaks plain HTTP and has no rate limit. Run it on a network you
  trust. Read `SECURITY.md` for the threat model.
- Hidden entries are not served unless `--hidden` or `hidden = true` is given.

[Unreleased]: https://github.com/ep9io/reposerve/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ep9io/reposerve/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ep9io/reposerve/releases/tag/v0.1.0
