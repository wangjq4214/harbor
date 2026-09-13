set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Show available development commands.
default:
    @just --list

# Build Harbor.
build:
    cargo build

# Run Harbor without widget hot reload.
run:
    cargo run

# Check every workspace target.
check:
    cargo check --workspace --all-targets

# Run the complete workspace test suite.
test:
    cargo test --workspace

# Format the workspace.
fmt:
    cargo fmt --all

# Verify formatting without modifying files.
fmt-check:
    cargo fmt --all -- --check

# Run Clippy with the same strict settings used by the project quality gate.
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run the ordinary non-HMR quality gates.
qa: fmt-check lint test

# Build the reloadable application UI DLL.
hmr-build:
    cargo build -p harbor-app-ui

# Rebuild the application UI DLL whenever reloadable composition changes.
# Run this in one terminal and `just hmr-run` in another.
hmr-watch: hmr-build
    cargo watch -w crates/harbor-app/src/ui.rs -w crates/harbor-app-ui/src -x "build -p harbor-app-ui"

# Build the UI DLL and run the persistent Runtime Host with hot reload enabled.
hmr-run: hmr-build
    cargo run --features widget-hot-reload

# Compile-check the Host's widget hot-reload path.
hmr-check:
    cargo check -p harbor --features widget-hot-reload

# Build the DLL and run Host tests with widget hot reload enabled.
hmr-test: hmr-build
    cargo test -p harbor --features widget-hot-reload

# Run ordinary and hot-reload verification.
qa-all: qa hmr-test

# Remove Cargo build artifacts.
clean:
    cargo clean
