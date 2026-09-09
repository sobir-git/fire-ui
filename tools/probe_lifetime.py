"""Signal-safe probe lifetimes. SIGKILL and power loss cannot run cleanup."""
import os
import signal
import subprocess
import time


def install_signal_cleanup():
    def interrupted(number, _frame):
        # Let finally blocks finish even if a terminal sends another signal.
        for item in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(item, signal.SIG_IGN)
        raise SystemExit(128 + number)

    for item in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(item, interrupted)


def run_owned(command, **kwargs):
    """Wait for a command; stop its whole process group before deleting scratch."""
    with subprocess.Popen(command, start_new_session=True, **kwargs) as process:
        try:
            return subprocess.CompletedProcess(command, process.wait())
        finally:
            # Descendants may still be alive even if their parent has exited.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            deadline = time.monotonic() + 5
            while True:
                try:
                    os.killpg(process.pid, 0)
                except ProcessLookupError:
                    break
                if time.monotonic() >= deadline:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    break
                time.sleep(.05)
            process.wait()
