"""Build isolated Release-equivalent Reader pump timings from production source.

Only pump_reader_with_after_read/service_reader_commands and their enums are copied.
Error logging is a no-op in this successful-read fixture; no test allocator is linked.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

RUST = r"""
#![allow(dead_code, unused_imports, unused_variables)]
use std::{
    io::Read,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError}},
    cell::Cell,
    hint::black_box,
    time::Instant,
};
macro_rules! bench_warn { ($($tokens:tt)*) => {}; }
mod tracing { pub(crate) use bench_warn as warn; }
const PTY_QUEUE_CAPACITY: usize = 32;
__CHUNK_MODULE__
__PRODUCTION__
struct NoRecycle;
impl NoRecycle {
    fn recycle<T>(&mut self, value: T) { drop(value); }
    fn flush(&mut self) {}
}
type BenchRecycle = __RECYCLE_TYPE__;

struct Chunks { remaining: usize, size: usize }
impl Read for Chunks {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 { return Ok(0); }
        self.remaining -= 1;
        bytes[..self.size].fill((self.remaining % 251) as u8);
        Ok(self.size)
    }
}
fn sample(count: usize, size: usize, batch: usize) -> f64 {
    let (tx, rx) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
    let (_commands, command_rx) = mpsc::channel();
    let pending = Arc::new(AtomicBool::new(false));
    let seen = Cell::new(0);
    __SETUP__
    let mut drain = || {
        while let Ok(event) = rx.try_recv() {
            let ReaderEvent::Bytes(bytes) = event else { panic!("unexpected ACK") };
            let expected = ((count - seen.get() - 1) % 251) as u8;
            assert_eq!(bytes.len(), size);
            assert_eq!(bytes.first(), Some(&expected));
            assert_eq!(bytes.last(), Some(&expected));
            black_box(&bytes[..]);
            seen.set(seen.get() + 1);
            recycle.recycle(bytes);
        }
        recycle.flush();
        pending.store(false, Ordering::Release);
    };
    let mut reads = 0;
    let start = Instant::now();
    pump_reader_with_after_read(
        Chunks { remaining: count, size }, tx, command_rx, pending.clone(), || true,
        || { reads += 1; if reads % batch == 0 { drain(); } }__BUFFER_ARG__,
    );
    drain();
    let ns = start.elapsed().as_nanos() as f64 / count as f64;
    assert_eq!(seen.get(), count);
    ns
}

fn sample_threaded(count: usize, size: usize) -> f64 {
    let (tx, rx) = mpsc::sync_channel(PTY_QUEUE_CAPACITY);
    let (_commands, command_rx) = mpsc::channel();
    let pending = Arc::new(AtomicBool::new(false));
    let worker_pending = pending.clone();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let worker_barrier = barrier.clone();
    __SETUP__
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        pump_reader_with_after_read(Chunks { remaining: count, size },
            tx, command_rx, worker_pending, || true, || {}__BUFFER_ARG__);
    });
    let mut seen = 0;
    let consume = |event: ReaderEvent, seen: &mut usize, recycle: &mut BenchRecycle| {
        let ReaderEvent::Bytes(bytes) = event else { panic!("unexpected ACK") };
        let expected = ((count - *seen - 1) % 251) as u8;
        assert_eq!(bytes.len(), size);
        assert_eq!(bytes.first(), Some(&expected));
        assert_eq!(bytes.last(), Some(&expected));
        black_box(&bytes[..]);
        *seen += 1;
        recycle.recycle(bytes);
    };
    let start = Instant::now();
    barrier.wait();
    while let Ok(event) = rx.recv() {
        consume(event, &mut seen, &mut recycle);
        while let Ok(event) = rx.try_recv() { consume(event, &mut seen, &mut recycle); }
        recycle.flush();
        pending.store(false, Ordering::Release);
    }
    worker.join().unwrap();
    let ns = start.elapsed().as_nanos() as f64 / count as f64;
    assert_eq!(seen, count);
    ns
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("threaded") {
        let chunks = 65_536;
        for size in [64, 4096] {
            black_box(sample_threaded(chunks, size));
            black_box(sample_threaded(chunks, size));
            let mut times: Vec<_> = (0..11).map(|_| sample_threaded(chunks, size)).collect();
            times.sort_by(f64::total_cmp);
            println!("TIMING reader chunk={size} batch=32 handoff=threaded chunks={chunks} samples=11 median_ns={:.3} batch_p95_ns={:.3} mib_per_sec={:.3} samples_ns={times:?}",
                times[5], times[10], size as f64 * 1e9 / times[5] / 1048576.0);
        }
        return;
    }
    let chunks = 131_072;
    for size in [64, 4096] {
        for batch in [1, 16] {
            black_box(sample(chunks, size, batch));
            black_box(sample(chunks, size, batch));
            let mut times: Vec<_> = (0..11).map(|_| sample(chunks, size, batch)).collect();
            times.sort_by(f64::total_cmp);
            println!("TIMING reader chunk={size} batch={batch} chunks={chunks} samples=11 median_ns={:.3} batch_p95_ns={:.3} mib_per_sec={:.3} samples_ns={times:?}",
                times[5], times[10], size as f64 * 1e9 / times[5] / 1048576.0);
        }
    }
}
"""

def extract(root: Path) -> tuple[str, dict]:
    io_path = root / "crates/harbor-terminal/src/io.rs"
    io = io_path.read_text(encoding="utf-8")
    enums = []
    for name in ("ReaderEvent", "ReaderCommand", "ReaderCommandStatus"):
        match = re.search(r"(?ms)^enum " + name + r" \{.*?^\}", io)
        if not match:
            raise ValueError(f"missing enum {name} in {io_path}")
        enums.append(match.group())
    pump = io.split("\nfn pump_reader_with_after_read", 1)[1].split(
        "\n/// Services at most one pending barrier", 1
    )[0]
    service = io.split("\nfn service_reader_commands", 1)[1].split(
        "\n// ── TerminalSession", 1
    )[0]
    production = "\n\n".join(enums + [
        "fn pump_reader_with_after_read" + pump,
        "fn service_reader_commands" + service,
    ])
    module = ""
    metadata = {"root": str(root), "io_sha256": hashlib.sha256(io.encode()).hexdigest()}
    if "Bytes(ReaderChunk)" in enums[0]:
        chunk = root / "crates/harbor-terminal/src/io/chunk.rs"
        module = '#[path = "' + chunk.as_posix() + '"]\nmod chunk;\nuse chunk::{ReaderBuffers, ReaderChunk};'
        metadata["chunk_sha256"] = hashlib.sha256(chunk.read_bytes()).hexdigest()
    explicit_recycle = "mut buffers: ReaderBuffers" in pump
    rust = RUST.replace("__CHUNK_MODULE__", module).replace("__PRODUCTION__", production)
    rust = rust.replace("__SETUP__", "let (buffers, mut recycle) = ReaderBuffers::pair();" if explicit_recycle else "let mut recycle = NoRecycle;")
    rust = rust.replace("__BUFFER_ARG__", ", buffers" if explicit_recycle else "")
    rust = rust.replace("__RECYCLE_TYPE__", "chunk::ReaderRecycle" if explicit_recycle else "NoRecycle")
    metadata["explicit_recycle"] = explicit_recycle
    metadata["generated_rust_sha256"] = hashlib.sha256(rust.encode()).hexdigest()
    return rust, metadata

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    metadata = {"rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()}
    for label, root in (("before", args.before.resolve()), ("after", args.after.resolve())):
        source, details = extract(root)
        path = out / f"{label}_reader.rs"
        path.write_text(source, encoding="utf-8")
        binary = out / f"{label}_reader.exe"
        command = ["rustc", "--edition=2024", "-Copt-level=3", "-Clto=fat",
                   "-Ccodegen-units=1", "-Cpanic=abort", str(path), "-o", str(binary)]
        subprocess.run(command, check=True)
        metadata[label] = details | {"binary": str(binary), "command": command}
        print(f"built {label}: {binary}", flush=True)
    (out / "reader-build.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")

if __name__ == "__main__":
    main()
