# The showcase

A small repository that uses every feature `reposerve` has. Serve it while you
work on the project.

```bash
cargo run --release -- -c config/reposerve.toml
```

Every type above is shown. The default filter shows Markdown alone, so pass the
list, or change it in the sidebar.

## What to look at

| Path | Shows |
| --- | --- |
| `markdown/prose.md` | Headings, tables, task lists, footnotes, callouts, math, a diagram, an image and fenced code. |
| `markdown/notes.md` | A short second page, so the previous and next links have a target. |
| `code/` | Six source types, highlighted: Rust, Python, JavaScript, SQL, TOML and JSON. |
| `notebooks/analysis.ipynb` | Markdown cells, math, a table, a chart, a diagram and a traceback. |
| `media/` | An SVG and a PNG, served as bytes. |
| `plain.txt` | A text file, shown as plain text. |

## Samples are real

Each file is a working example of its format. A change here changes what the
screenshots and the tests show, so keep it small.
