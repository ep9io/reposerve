# Vendored assets

## mermaid.min.js

| Field | Value |
| --- | --- |
| Package | `mermaid` 11.17.2, `dist/mermaid.min.js` |
| Source | `npm pack mermaid@11.17.2` from the npm registry |
| Licence | MIT. The notice is in `mermaid.LICENSE`, as the licence requires |
| SHA-256 | `581ed7d74bd9048d0e3a91363927d72ef22942d7722546b27f7cc29e35390eb8` |
| Size | 3.4 MiB, 953 KiB after gzip |

The file is embedded in the `docsite` binary at compile time, so the server needs
no network and no Node toolchain. It is sent only to pages that contain a Mermaid
diagram, and with a one-year cache lifetime.

To update it, run `npm pack mermaid@<version>`, replace the file and this table,
and keep the licence notice beside it.
