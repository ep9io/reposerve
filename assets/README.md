# Vendored assets

Every file here is embedded in the binary at build time. The server needs no
network and no Node toolchain at run time.

## Layout

| Path | What it holds |
| --- | --- |
| `app.css`, `app.js`, `icon.svg` | Written for this project. Edit them in place. |
| `mermaid/` | The diagram bundle and its licence notice. |
| `katex/` | The math script, its stylesheet, its fonts and its licence notice. |

One directory per bundled package, so a notice always sits beside the files it
covers. Our own files stay at the top, because they are not a package.

## `mermaid/`

| Field   | Value                                                        |
| ------- | ------------------------------------------------------------ |
| Package | `mermaid` 11.17.2, `dist/mermaid.min.js`                     |
| Source  | `npm pack mermaid@11.17.2` from the npm registry              |
| Licence | MIT. The notice is in `mermaid/LICENSE`.                      |
| SHA-256 | `581ed7d74bd9048d0e3a91363927d72ef22942d7722546b27f7cc29e35390eb8` |
| Size    | 3.4 MiB                                                       |

The file is sent only to a page that holds a diagram, from a URL that carries
the fingerprint of its content. The browser keeps it for a year.

## `katex/`

| Field    | Value                                                        |
| -------- | ------------------------------------------------------------ |
| Package  | `katex` 0.19.1, `dist/katex.min.js` and `dist/katex.min.css` |
| Source   | `npm pack katex@0.19.1` from the npm registry                 |
| Licence  | MIT. The notice is in `katex/LICENSE`.                        |
| JS size  | 268 KiB. SHA-256 `fa140498b8cfc8302cbe5e7cb1bdbc1f952bc38b3fa258ede09e73edbf9a74bc` |
| CSS size | 28 KiB. SHA-256 `00e0b094171b07f58422075f469b4feac2b2f7bce1aeec943c08da4d697f4dcf` |

`katex/fonts/` holds the 20 WOFF2 fonts, 296 KiB in total. The stylesheet also
names WOFF and TTF copies. Every browser that runs the script reads WOFF2 first,
so those formats are not shipped. Their names resolve to a 404, and the browser
does not ask for them.

The script and the stylesheet are sent only to a page that holds math.

## `app.css`, `app.js`, `icon.svg`

Written for this project. Their URLs carry a fingerprint, so a change reaches
every reader without a cache flush.

## Updating a package

1. Run `npm pack <package>@<version>` in a temporary directory.
2. Replace the files in the package directory and the table above.
3. Keep the licence notice beside the files.
4. Run `just check`.
5. Open `showcase/markdown/prose.md` and confirm that the diagram and the math
   render.
