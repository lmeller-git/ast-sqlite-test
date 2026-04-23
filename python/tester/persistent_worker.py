from dataclasses import dataclass, field
import asyncio
import time
import os
import tempfile
import shutil


@dataclass(order=True)
class TestCapture:
    stdout: bytes = field(compare=False)
    stderr: bytes = field(compare=False)
    exit_code: int | None = field(compare=False)
    query: str = field(compare=False)
    exec_time: int
    is_hang_or_crash: None | str = field(compare=False)

    def __format__(self, format_spec: str) -> str:
        return f"TestCapture {{\nstdout: {self.stdout.decode(errors='replace')}\nstdserr: {self.stderr.decode(errors='replace')}\nexit_code: {self.exit_code}\nexec_time:{self.exec_time}\nquery: {self.query}\n}}"


class SQLiteWorker:
    def __init__(self, proc_path: str, env: dict[str, str] | None = None):
        self.db_path: str = ":memory:"
        self.proc_path: str = proc_path
        self.proc: asyncio.subprocess.Process | None = None
        self.env: dict[str, str] | None = env
        self.STDOUT_SENTINEL: bytes = b"__STDOUT_EOQ__"
        self.STDERR_SENTINEL: bytes = b"__STDERR_EOQ__"
        self.workdir: str = tempfile.mkdtemp(prefix="sqlite_worker_")

    async def _start(self) -> None:
        if self.proc is not None:
            if self.proc.returncode is None:
                self.proc.kill()
            _ = await self.proc.wait()
            self.proc = None

        full_env = os.environ.copy()
        if self.env is not None:
            full_env.update(self.env)

        self.proc = await asyncio.create_subprocess_exec(
            self.proc_path,
            self.db_path,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=full_env,
            cwd=self.workdir
        )

    async def _read_until_sentinel(
        self, stream: asyncio.StreamReader, sentinel: bytes
    ) -> tuple[bytes, bool]:
        chunks: list[bytes] = []
        while True:
            # TODO: handle StreamOverrunError
            line = await stream.readline()
            if not line:
                return b"".join(chunks), False
            if line.strip() == sentinel:
                return b"".join(chunks), True
            chunks.append(line)

    async def execute(self, query: str, timeout_sec: float = 1.5) -> TestCapture:
        if self.proc is None or self.proc.returncode is not None:
            await self._start()

        _ = await self.reset()

        stdout_stream: asyncio.StreamReader = self.proc.stdout
        stderr_stream: asyncio.StreamReader = self.proc.stderr

        start_time = time.perf_counter_ns()
        full_command = f"{query};\n.shell echo {self.STDERR_SENTINEL.decode()} >&2\n.print {self.STDOUT_SENTINEL.decode()}\n"

        try:
            self.proc.stdin.write(full_command.encode())
            await self.proc.stdin.drain()

            (stdout_bytes, stdout_ok), (stderr_bytes, stderr_ok) = await asyncio.wait_for(
                asyncio.gather(
                    self._read_until_sentinel(stdout_stream, self.STDOUT_SENTINEL),
                    self._read_until_sentinel(stderr_stream, self.STDERR_SENTINEL),
                ),
                timeout=timeout_sec,
            )
            exec_time = time.perf_counter_ns() - start_time

            # TestCapture construction

            # process died
            if not stdout_ok or not stderr_ok:
                exit_code = await self.proc.wait()
                self.proc = None
                return TestCapture(
                    stdout=stdout_bytes,
                    stderr=stderr_bytes,
                    exit_code=exit_code,
                    query=query,
                    exec_time=exec_time,
                    is_hang_or_crash="CRASH",
                )

            # process died AFTER both .print
            if self.proc.returncode is not None:
                exit_code = self.proc.returncode
                self.proc = None
                return TestCapture(
                    stdout=stdout_bytes,
                    stderr=stderr_bytes,
                    exit_code=exit_code,
                    query=query,
                    exec_time=exec_time,
                    is_hang_or_crash="CRASH",
                )

            # process is alive, query ran
            # need to restore a clean state via kill, since ATTACHED dbs dont get wiped on .open :memory:
            if "ATTACH" in query:
                self.proc.kill()
                _ = await self.proc.wait()
                self.proc = None
                for f in os.scandir(self.workdir):
                    try:
                        os.remove(f.path) if f.is_file() else shutil.rmtree(f.path)
                    except OSError:
                        pass

            return TestCapture(
                stdout=stdout_bytes,
                stderr=stderr_bytes,
                exit_code=0,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash=None,
            )

        except asyncio.TimeoutError:
            try:
                self.proc.kill()
                _ = await self.proc.wait()
            except ProcessLookupError:
                print("couldnt kill timed out process", flush=True)
            self.proc = None
            exec_time = time.perf_counter_ns() - start_time
            return TestCapture(
                stdout=b"",
                stderr=b"EXECUTION TIMEOUT EXCEEDED",
                exit_code=42,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash="HANG",
            )

        except Exception as e:
            print(f"exception {str(e)}\n", flush=True)
            # should never happen
            if self.proc is not None:
                try:
                    self.proc.kill()
                    _ = await self.proc.wait()
                except ProcessLookupError:
                    print("couldnt kill process after exception", flush=True)
            self.proc = None
            exec_time = time.perf_counter_ns() - start_time
            return TestCapture(
                stdout=b"",
                stderr=f"\npython Exception: {str(e)}".encode(),
                exit_code=-1,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash="CRASH",
            )

    async def reset(self) -> None:
        if self.proc is not None and self.proc.returncode is None:
            self.proc.stdin.write(f".open {self.db_path}\n".encode())
            await self.proc.stdin.drain()

    async def close(self) -> None:
        if self.proc is not None and self.proc.returncode is None:
            try:
                self.proc.stdin.write(b".quit\n")
                await self.proc.stdin.drain()
                _ = await self.proc.wait()
            except Exception:
                self.proc.kill()
                _ = await self.proc.wait()
        self.proc = None
        shutil.rmtree(self.workdir, ignore_errors=True)
