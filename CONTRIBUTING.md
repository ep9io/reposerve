# Contributing

`reposerve` is a small project. Keep each change small and keep the checks green.

## Before you send a change

1. Run the whole gate.

   ```bash
   just check
   ```

   The gate runs formatting, lints, tests and the dependency policy.
2. Open the sample site. Check the page you changed on a narrow window as well.

   ```bash
   cargo run --release -- -c config/reposerve.toml
   ```

## What the gate checks

| Recipe         | Checks                                             |
| -------------- | -------------------------------------------------- |
| `just fmt`     | Formatting                                         |
| `just check`   | Formatting, lints, tests and the dependency policy |
| `just package` | A publish dry run                                  |


## Releases

A release is four small steps.

1. Give the entries under `Unreleased` in `CHANGELOG.md` a version and a date.
2. Put the same version in `Cargo.toml`.
3. Run `cargo publish` from that commit.
4. Tag the commit `v<version>` and push the tag.

The tag does the rest. It runs the gate once more, builds one archive for each
platform (Linux x86_64 and aarch64, macOS x86_64 and aarch64, Windows x86_64),
and attaches every archive with its `.sha256` file to a GitHub release.

## Layout

| Path                            | Holds                                                                                      |
| ------------------------------- | ------------------------------------------------------------------------------------------ |
| `src/site/`                     | What a repository is: the file index, Markdown, highlighting, notebooks, media.            |
| `src/web/`                      | How a page is served: the socket, the routes, the assets, the caches and the theme cookie. |
| `src/ui/`                       | The markup of a page: the shell, the sidebar, the contents and the views.                  |
| `src/config.rs`, `src/error.rs` | The public configuration and the typed errors.                                             |
| `assets/`                       | The vendored packages and the files written for this project.                              |
| `showcase/`                     | A small repository that uses every feature.                                                |

A module that renders a file does not know about a request. A module that answers
a request does not build HTML by hand.

## Assets

`assets/` holds one directory per vendored package, beside `app.css`, `app.js` and
`icon.svg`. Read `assets/README.md` before you change one. The binary embeds those
files, so a changed asset needs a rebuild.
