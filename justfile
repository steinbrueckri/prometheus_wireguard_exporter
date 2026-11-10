# Local developer workflows. The GitHub Actions pipeline calls the same recipes,
# so "just ci" locally checks exactly what the pipeline checks.

binary := "prometheus_wireguard_exporter"

# List the available recipes
default:
    @just --list

# Build a debug binary
build:
    cargo build --locked

# Check the formatting without changing files
fmt-check:
    cargo fmt --check

# Lint, warnings are errors
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Run the unit tests
test:
    cargo test --locked

# Everything the pipeline verifies
ci: fmt-check lint test

# Build a release binary plus checksum into dist/ (the musl targets need Linux with musl-tools)
dist target:
    rustup target add {{target}}
    cargo build --locked --release --target {{target}}
    mkdir -p dist
    cp target/{{target}}/release/{{binary}} dist/{{binary}}_{{target}}
    cd dist && sha256sum {{binary}}_{{target}} > {{binary}}_{{target}}.sha256

# Fail unless the given tag matches the version in Cargo.toml
check-version tag:
    @version="$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[0].version')"; \
    if [ "{{tag}}" != "$version" ]; then \
        echo "tag {{tag}} does not match the Cargo.toml version $version" >&2; \
        exit 1; \
    fi
