"""Run already-built hot-path timings in alternating order, without concurrent builds."""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import subprocess
import time

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("target/hot-path-timing"))
    parser.add_argument("--stage", choices=("reader", "reader-threaded", "engine", "sequence"), required=True)
    parser.add_argument("--rounds", type=int, default=4)
    parser.add_argument("--affinity", type=lambda value: int(value, 0), default=1)
    args = parser.parse_args()
    out = args.out.resolve()
    results = out / "results"
    results.mkdir(parents=True, exist_ok=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    metadata = {"stage": args.stage, "affinity_mask": args.affinity, "runs": []}
    for round_index in range(args.rounds):
        order = ("before", "after") if round_index % 2 == 0 else ("after", "before")
        for label in order:
            if args.stage == "reader-threaded":
                cases = (("threaded",),)
            elif args.stage == "reader":
                cases = (("reader",),)
            elif args.stage == "engine":
                cases = (("engine",),)
            else:
                cases = (("sequence", "24"), ("sequence", "80"))
            for case in cases:
                binary = out / (f"{label}_reader.exe" if args.stage.startswith("reader") else f"{label}.exe")
                command = [str(binary)] + ([] if args.stage == "reader" else list(case))
                log = results / f"{args.stage}-{'-'.join(case[1:]) or 'all'}-{label}-{round_index}.log"
                started = time.time()
                with log.open("w", encoding="utf-8") as stream:
                    process = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT)
                    handle = kernel.OpenProcess(0x0200, False, process.pid)
                    if not handle:
                        process.terminate()
                        raise ctypes.WinError(ctypes.get_last_error())
                    try:
                        if not kernel.SetProcessAffinityMask(handle, args.affinity):
                            process.terminate()
                            raise ctypes.WinError(ctypes.get_last_error())
                    finally:
                        kernel.CloseHandle(handle)
                    code = process.wait()
                record = {"label": label, "round": round_index, "case": case,
                          "command": command, "log": str(log), "exit_code": code,
                          "elapsed_seconds": time.time() - started,
                          "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
                metadata["runs"].append(record)
                (results / f"{args.stage}-runs.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
                print(f"{args.stage} round={round_index} {label} {' '.join(case)} exit={code} wall={record['elapsed_seconds']:.2f}s log={log}", flush=True)
                if code:
                    raise RuntimeError(f"benchmark failed; inspect {log}")
    print(f"completed {len(metadata['runs'])} processes", flush=True)

if __name__ == "__main__":
    main()
