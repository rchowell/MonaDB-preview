"""Start and stop a MonaDB server subprocess for e2e tests."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

LISTENING_RE = re.compile(r"MonaDB listening on (?P<addr>\S+)")


@dataclass
class MonaDBServer:
    process: subprocess.Popen[str]
    addr: str

    @property
    def uri(self) -> str:
        return f"mongodb://{self.addr}"

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def start(bind_addr: str = "127.0.0.1:0", timeout_s: float = 30.0) -> MonaDBServer:
    """Build and start monadb, returning once it is accepting connections."""
    root = repo_root()
    binary = os.environ.get("MONADB_BIN")

    if binary:
        cmd = [binary, "--addr", bind_addr]
    else:
        cmd = ["cargo", "run", "--quiet", "--", "--addr", bind_addr]

    process = subprocess.Popen(
        cmd,
        cwd=root if not binary else None,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    assert process.stdout is not None
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        line = process.stdout.readline()
        if not line:
            if process.poll() is not None:
                break
            continue

        print(f"[monadb] {line.rstrip()}")
        match = LISTENING_RE.search(line)
        if match:
            return MonaDBServer(process=process, addr=match.group("addr"))

    output = process.stdout.read() if process.stdout else ""
    if process.poll() is None:
        process.kill()
    raise RuntimeError(f"MonaDB failed to start within {timeout_s}s\n{output}")


def main() -> None:
    server = start()
    print(f"Started MonaDB at {server.uri}", file=sys.stderr)
    try:
        server.process.wait()
    except KeyboardInterrupt:
        pass
    finally:
        server.stop()
