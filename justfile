# reposerve - task runner.
#
# `just check` is the whole gate: formatting, lints, tests, the dependency
# policy and a package dry run. Run it before a release.

# List the available recipes.
default:
    @just --list

# Serve the current directory.
serve *args:
    cargo run --release -- {{args}}

# Format the code.
fmt:
    cargo fmt --all

# Check formatting, lints and tests.
check:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo deny check

# Build the package the way a publish would, without uploading it.
package:
    cargo publish --dry-run --allow-dirty
