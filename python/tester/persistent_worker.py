from dataclasses import dataclass, field
import asyncio
from pathlib import Path
# import signal
import time
import os
import tempfile
import shutil
from typing import override
import itertools


@dataclass(order=True)
class TestCapture:
    stdout: bytes = field(compare=False)
    stderr: bytes = field(compare=False)
    exit_code: int | None = field(compare=False)
    query: str = field(compare=False)
    exec_time: int
    is_hang_or_crash: None | str = field(compare=False)

    @override
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
        self.workdir: str = tempfile.mkdtemp(prefix="sqlite_worker_", dir="/dev/shm")
        self._reset_counter: itertools.count[int] = itertools.count()

    async def _start(self) -> None:
        if self.proc is not None:
            if self.proc.returncode is None:
                self.proc.kill()
            try:
                _ = await asyncio.wait_for(self.proc.wait(), 0.1)
            except TimeoutError:
                pass
            self.proc = None

        full_env = os.environ.copy()
        full_env["ASAN_OPTIONS"] = "log_path=sdterr"
        full_env["UBSAN_OPTIONS"] = "log_path=stderr:print_stacktrace=1"
        if self.env is not None:
            full_env.update(self.env)

        self.proc = await asyncio.create_subprocess_exec(
            self.proc_path,
            self.db_path,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=full_env,
            cwd=self.workdir,
        )

    async def _read_until_sentinel(
        self, stream: asyncio.StreamReader, sentinel: bytes
    ) -> tuple[bytes, bool]:
        try:
            data = await stream.readuntil(sentinel)
            return data, True
        except asyncio.IncompleteReadError as e:
            return e.partial, False
        except asyncio.LimitOverrunError as e:
            data = await stream.read(e.consumed)
            return data, False

    async def execute(self, query: str, timeout_sec: float = 1.0) -> TestCapture:
        if self.proc is None or self.proc.returncode is not None:
            await self._start()

        stdout_stream: asyncio.StreamReader = self.proc.stdout
        stderr_stream: asyncio.StreamReader = self.proc.stderr

        start_time = time.perf_counter_ns()
        full_command = f"\n;\n{query}\n;\n.output stderr\n.print {self.STDERR_SENTINEL.decode()}\n.output stdout\n.print {self.STDOUT_SENTINEL.decode()}\n"

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
                exit_code = await asyncio.wait_for(self.proc.wait(), 0.2)
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

            return TestCapture(
                stdout=stdout_bytes,
                stderr=stderr_bytes,
                exit_code=0,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash=None,
            )

        except asyncio.TimeoutError:
            # in theory SIGINT should be able to restore most hangs, however in practice we need to restart to prevent state leakage.
            await self.hard_reset()
            exec_time = time.perf_counter_ns() - start_time
            return TestCapture(
                stdout=b"",
                stderr=b"EXECUTION TIMEOUT EXCEEDED",
                exit_code=42,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash="HANG",
            )
            # err_output = b""
            # try:
            #     self.proc.send_signal(signal.SIGINT)
            # except ProcessLookupError:
            #     # already dead
            #     stdout_remaining, stderr_remaining = await self.proc.communicate()
            #     exec_time = time.perf_counter_ns() - start_time
            #     await self.hard_reset()

            #     return TestCapture(
            #         stdout=stdout_remaining,
            #         stderr=stderr_remaining,
            #         exit_code=self.proc.returncode or -1,
            #         query=query,
            #         exec_time=exec_time,
            #         is_hang_or_crash="CRASH",
            #     )

            # try:
            #     err_output = await asyncio.wait_for(self.proc.stderr.readline(), timeout=0.1)
            #     if b"interrupted" in err_output:
            #         # interrupted actual hang
            #         await self.hard_reset()
            #         pass
            #     else:
            #         # maybe it just finished on its own, or some other error
            #         await self.hard_reset()
            # except asyncio.TimeoutError:
            #     # SIGINT did not work. likely because of an unclosed qoute upstream
            #     #  migth want to reutnr sth indicating a syntax err instead TODO
            #     await self.hard_reset()

            # exec_time = time.perf_counter_ns() - start_time
            # return TestCapture(
            #     stdout=b"",
            #     stderr=b"EXECUTION TIMEOUT EXCEEDED, " + err_output,
            #     exit_code=42,
            #     query=query,
            #     exec_time=exec_time,
            #     is_hang_or_crash="HANG",
            # )

        except Exception as e:
            print(f"exception {str(e)}\n", flush=True)
            # self.proc.send_signal(signal.SIGINT)
            # try:
            #     err_output = await asyncio.wait_for(self.proc.stderr.readline(), timeout=0.1)
            #     if b"interrupted" in err_output:
            #         # interrupted actual hang
            #        pass
            #     else:
            #         # maybe it just finished on its own, or some other error
            #         pass
            # except asyncio.TimeoutError:
            #     # SIGINT did not work. likely because of an unclosed qoute upstream
            #     #  migth want to reutnr sth indicating a syntax err instead TODO
            #     #await self.hard_reset()
            #     pass

            exec_time = time.perf_counter_ns() - start_time
            return TestCapture(
                stdout=b"",
                stderr=f"\npython Exception: {str(e)}".encode(),
                exit_code=-1,
                query=query,
                exec_time=exec_time,
                is_hang_or_crash="CRASH",
            )
        finally:
            await self.reset()

    async def reset(self) -> None:
        if self.proc is not None and self.proc.returncode is None:
            epoch = next(self._reset_counter)
            if epoch % 50 == 0:
                await self.hard_reset()
                return
            marker = f"__RESET_{epoch}__".encode()
            reset_cmd = f".open {self.db_path}\n.bail off\n.log off\n.output stderr\n.print {marker.decode()}\n.output stdout\n.print {marker.decode()}\n"
            self.proc.stdin.write(reset_cmd.encode())
            await self.proc.stdin.drain()
            try:
                _ = await asyncio.wait_for(
                    asyncio.gather(
                        self._read_until_sentinel(self.proc.stderr, marker),
                        self._read_until_sentinel(self.proc.stdout, marker),
                    ),
                    timeout=0.5,
                )
            except asyncio.TimeoutError:
                await self.hard_reset()
        self.clear_workdir_contents()

    async def hard_reset(self) -> None:
        try:
            self.proc.kill()
            _ = await self.proc.wait()
        except ProcessLookupError:
            print("couldnt kill timed out process", flush=True)
        self.proc = None

    def clear_workdir_contents(self):
        path = Path(self.workdir)
        for child in path.iterdir():
            if child.is_file() or child.is_symlink():
                child.unlink()
            elif child.is_dir():
                shutil.rmtree(child)

    async def close(self) -> None:
        if self.proc is not None and self.proc.returncode is None and not self.proc.stdin.is_closing():
            try:
                self.proc.stdin.write(b".quit\n")
                await self.proc.stdin.drain()
                _ = await self.proc.wait()
            except Exception:
                self.proc.kill()
                _ = await self.proc.wait()
        self.proc = None
        shutil.rmtree(self.workdir, ignore_errors=True)
