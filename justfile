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

# Run the complete workspace test suite.
test:
    cargo test --workspace

# Test one crate, optionally filtering by test name: just test-crate harbor-parser parse.
test-crate crate filter="":
    cargo test -p "{{ crate }}" "{{ filter }}"

# Run Python script unit tests without launching Harbor or touching the clipboard.
script-test:
    python -B -m unittest discover -s scripts/tests -v

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

# Summarize the terminal protocol inventory (not release acceptance).
checklist:
    python scripts/docs/checklist_summary.py

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

# Run ordinary and hot-reload verification.
qa-all: qa hmr-test

# Remove Cargo build artifacts.
clean:
    cargo clean
