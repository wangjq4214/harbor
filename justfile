set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Show available development commands.
default:
    @just --list

# Restore the pinned ConPTY runtime (Windows; run after cloning or pin changes).
[windows]
setup:
    powershell.exe -NoLogo -NoProfile -File scripts/setup/fetch_conpty.ps1

# Build Harbor.
build:
    cargo build

# Build an optimized development binary (use package for distributable assets).
build-release:
    cargo build --release

# Run Harbor without widget hot reload.
run:
    cargo run

# Run Harbor with release optimizations, without widget hot reload.
run-release:
    cargo run --release

# Check every workspace target.
check:
    cargo check --workspace --all-targets

# Run default (no GPU/PTY/desktop) tests and the renderer-free terminal contract.
test:
    cargo test --locked --workspace
    cargo test --locked -p harbor-terminal --no-default-features

# Source ownership guards, separate from behavioral cases (also included in test).
test-architecture:
    cargo test --locked --workspace architecture::

# Required offscreen GPU tests; select a backend with HARBOR_TEST_GPU.
test-gpu:
    cargo test --locked --workspace gpu:: -- --ignored --test-threads=1

# Real ConPTY child processes; restore the pinned runtime first.
[windows]
test-pty:
    cargo test --locked --workspace pty:: -- --ignored --test-threads=1

# Opt-in desktop, installed-font and external-application acceptance.
[windows]
test-native filter="native::":
    cargo test --locked --workspace "{{ filter }}" -- --ignored --test-threads=1 --nocapture

# Opt-in release measurements, not correctness gates.
test-perf:
    cargo test --locked -p harbor-terminal --release perf:: -- --ignored --test-threads=1 --nocapture

# Test one crate, optionally filtering by test name: just test-crate harbor-parser parse.
test-crate crate filter="":
    cargo test -p "{{ crate }}" "{{ filter }}"

# Run Python script unit tests without launching Harbor or touching the clipboard.
script-test:
    python -B -m unittest discover -s scripts/tests -v

# Smoke-test the Windows process sampler itself, not Harbor.
[windows]
sampler-test:
    pwsh -NoProfile -File scripts/tests/test_sample_process.ps1

# Run packaging rejection tests (no install or release payload required).
[windows]
package-test:
    powershell.exe -NoLogo -NoProfile -File scripts/tests/test_package_windows.ps1

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

# Generate workspace API documentation without dependency docs.
doc:
    cargo doc --workspace --no-deps

# Check repository documentation links and consistency.
docs-check:
    python scripts/docs/check_docs.py

# Validate the root version for Windows packaging without building artifacts.
[windows]
package-check:
    powershell.exe -NoLogo -NoProfile -File scripts/packaging/package_windows.ps1 -ValidateVersionOnly

# Build validated Windows release assets: Both (default), Msi, or Zip; ZIP needs no WiX.
[windows]
package format="Both":
    powershell.exe -NoLogo -NoProfile -File scripts/packaging/package_windows.ps1 -Format "{{ format }}"

# Capture a DHAT heap profile; close Harbor normally to write dhat-heap.json.
dhat-run:
    cargo run --profile dhat --features dhat-heap

# Summarize an existing DHAT report, optionally supplying its path.
dhat-analyze report="dhat-heap.json":
    python scripts/profiling/dhat_analyze.py "{{ report }}"

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
    cargo check -p harbor-widget --features hmr
    cargo check -p harbor --features widget-hot-reload
    cargo check -p harbor --features widget-hot-reload --release

# Build the DLL and run Host tests with widget hot reload enabled.
hmr-test: hmr-build
    cargo test -p harbor-widget --features hmr
    cargo test -p harbor --features widget-hot-reload

# Run automated resource suites and hot-reload verification; no desktop/manual fixtures.
[windows]
qa-all: qa test-gpu test-pty script-test sampler-test hmr-test

# Remove Cargo build artifacts.
clean:
    cargo clean
