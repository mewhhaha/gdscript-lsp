set shell := ["bash", "-euo", "pipefail", "-c"]

# Install gdscript-lsp and update Helix to use it.
install:
    cargo install --path .
    scripts/setup_helix.sh
