"""Run with python3 -m unittest discover -s tools -p 'test_*.py'."""
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import build_storage as storage
import lean_probe
from probe_lifetime import install_signal_cleanup


def setUpModule():
    install_signal_cleanup()

TOOLS = Path(__file__).resolve().parent


class StorageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='fire-ui-storage-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        self.target = self.root / 'target'
        self.target.mkdir()
        (self.target / '.rustc_info.json').write_text(json.dumps({'rustc_fingerprint': 1}))
        (self.target / 'debug').mkdir()
        (self.target / 'debug/.cargo-lock').touch()
        (self.target / 'debug/generated').write_text('build product')

    def test_refuses_broad_foreign_and_symlink_paths(self):
        for path in (Path('/'), self.root, self.root / 'elsewhere', self.target / 'debug'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                storage.validate_target(self.root, path)
        foreign = self.root / 'foreign'
        foreign.mkdir()
        (self.target / 'release').symlink_to(foreign, target_is_directory=True)
        with self.assertRaises(ValueError):
            storage.clean(self.root)
        self.assertTrue((self.target / 'debug/generated').exists())
        (self.target / 'release').unlink()
        self.target.rename(self.root / 'saved')
        self.target.symlink_to(self.root / 'saved', target_is_directory=True)
        with self.assertRaises(ValueError):
            storage.clean(self.root)

    def test_refuses_missing_marker_and_tracked_products(self):
        marker = self.target / '.rustc_info.json'
        marker.unlink()
        with self.assertRaises(ValueError):
            storage.clean(self.root)
        marker.write_text(json.dumps({'rustc_fingerprint': 1}))
        subprocess.run(['git', '-C', str(self.root), 'add', 'target/debug/generated'], check=True)
        with self.assertRaises(ValueError):
            storage.clean(self.root)

    def test_clean_preserves_sources_evidence_and_lock_inode(self):
        source = self.root / 'source.rs'
        source.write_text('source')
        evidence = self.root / 'artifacts'
        evidence.mkdir()
        (evidence / 'result').write_text('evidence')
        lock = self.target / 'debug/.cargo-lock'
        inode = lock.stat().st_ino
        storage.clean(self.root)
        storage.clean(self.root)
        self.assertEqual(lock.stat().st_ino, inode)
        self.assertFalse((self.target / 'debug/generated').exists())
        self.assertEqual(source.read_text(), 'source')
        self.assertEqual((evidence / 'result').read_text(), 'evidence')

    def test_busy_lock_refuses_without_partial_cleanup(self):
        with (self.target / 'debug/.cargo-lock').open('r+') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(ValueError):
                storage.clean(self.root)
        self.assertTrue((self.target / 'debug/generated').exists())

    def test_preflight_rejects_size_and_low_space(self):
        with self.assertRaises(ValueError):
            storage.preflight([self.root], max_target=1, min_free=0)
        with self.assertRaises(ValueError):
            storage.preflight([self.root], min_free=2**80)

    def test_lean_build_lifetime(self):
        for fails in (False, True):
            scratch = []
            def build(command, cwd, env, **kwargs):
                project = Path(cwd)
                target = Path(env['CARGO_TARGET_DIR'])
                scratch.append(project.parent)
                binary = target / 'host/release/fire-ui-lean-probe'
                binary.parent.mkdir(parents=True)
                binary.write_bytes(b'evidence binary')
                (project / 'Cargo.lock').write_text('lock')
                return subprocess.CompletedProcess(command, int(fails))
            with patch.object(lean_probe, 'run_owned', build), patch.object(lean_probe, 'output', return_value='fire-ui 0.7.0'):
                if fails:
                    with self.assertRaises(RuntimeError):
                        lean_probe.build('minimal', self.root, 'host')
                else:
                    binary, _ = lean_probe.build('minimal', self.root, 'host')
                    self.assertTrue(binary.is_file())
            self.assertTrue(scratch)
            self.assertFalse(scratch[0].exists())

    def test_temp_cleanup_on_exit_failure_and_signals(self):
        for ending in ('success', 'failure', 'SIGINT', 'SIGTERM', 'SIGHUP'):
            report = self.root / 'scratch-path'
            report.unlink(missing_ok=True)
            code = '''
import pathlib, sys, tempfile, time
from probe_lifetime import install_signal_cleanup, run_owned
install_signal_cleanup()
with tempfile.TemporaryDirectory(prefix="fire-ui-lifetime-test-") as scratch:
    pathlib.Path(sys.argv[1]).write_text(scratch)
    if sys.argv[2] == 'failure':
        raise RuntimeError('intentional')
    if sys.argv[2] != 'success':
        run_owned([sys.executable, '-c', 'import time; time.sleep(60)'])
'''
            process = subprocess.Popen([sys.executable, '-c', code, str(report), ending],
                                       env={**os.environ, 'PYTHONPATH': str(TOOLS)}, stderr=subprocess.DEVNULL)
            try:
                deadline = time.monotonic() + 5
                while not report.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(report.exists())
                scratch = Path(report.read_text())
                if ending.startswith('SIG'):
                    time.sleep(.1)
                    process.send_signal(getattr(signal, ending))
                process.wait(timeout=8)
                self.assertEqual(process.returncode == 0, ending == 'success')
                self.assertFalse(scratch.exists(), ending)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()



    def test_documented_shell_scratch_cleanup(self):
        document = (TOOLS.parent / 'docs/inspection.md').read_text()
        snippet = document.split('```sh\n', 1)[1].split('```', 1)[0]
        command = 'FIRE_UI_INSPECT="$inspection_dir/ui.sock" cargo run --release -p fire-ui-studio'
        for ending in ('true', 'false', 'sleep 30'):
            shell_temp = self.root / 'shell-temp'
            shell_temp.mkdir(exist_ok=True)
            process = subprocess.Popen(
                ['sh', '-c', snippet.replace(command, ending)],
                env={**os.environ, 'TMPDIR': str(shell_temp)}, start_new_session=True,
                stderr=subprocess.DEVNULL)
            try:
                if ending == 'sleep 30':
                    deadline = time.monotonic() + 5
                    while not list(shell_temp.iterdir()):
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(.01)
                    os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
                self.assertEqual(list(shell_temp.iterdir()), [])
                self.assertEqual(process.returncode == 0, ending == 'true')
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()

    def test_real_cargo_concurrency_and_cleanup_lock(self):
        with tempfile.TemporaryDirectory(prefix='fire-ui-lock-test-') as temporary:
            root=Path(temporary)
            for name in ['a','b']:
                project=root/name
                (project/'src').mkdir(parents=True)
                (project/'Cargo.toml').write_text('[package]\nname="lock-check"\nversion="0.1.0"\nedition="2021"\n')
                (project/'src/main.rs').write_text('fn main() {}')
                (project/'build.rs').write_text('fn main() { if let Ok(p) = std::env::var("WAIT_FILE") { std::fs::write(format!("{p}.ready"), "").unwrap(); while !std::path::Path::new(&p).exists() { std::thread::sleep(std::time::Duration::from_millis(20)); } } }')
                subprocess.run(['git','init','-q',str(project)],check=True)
            subprocess.run(['cargo','build','--offline'],cwd=root/'a',check=True,capture_output=True)
            with (root/'a/build.rs').open('a') as source: source.write('\n')
            gate=root/'gate'
            log=(root/'waiting.log').open('w+')
            first=subprocess.Popen(['cargo','build','--offline'],cwd=root/'a',env={**os.environ,'WAIT_FILE':str(gate)},stdout=log,stderr=log)
            second=None
            try:
                deadline=time.monotonic()+15
                while not Path(str(gate)+'.ready').exists():
                    if time.monotonic()>deadline: raise RuntimeError('build script did not start')
                    time.sleep(.02)
                try:
                    storage.clean(root/'a')
                    raise AssertionError('cleanup accepted active Cargo')
                except ValueError as error:
                    assert 'Active Cargo build' in str(error),str(error)
                second=subprocess.Popen(['cargo','build','--offline','--target-dir',str(root/'a/target')],cwd=root/'b',stdout=log,stderr=log)
                deadline = time.monotonic() + 5
                while True:
                    log.seek(0)
                    if 'Blocking waiting for file lock' in log.read():
                        break
                    self.assertLess(time.monotonic(), deadline)
                    time.sleep(.05)
                subprocess.run(['cargo','build','--offline'],cwd=root/'b',check=True,timeout=15,capture_output=True)
                assert first.poll() is None
            finally:
                gate.touch()
                first.wait(timeout=15)
                if second: second.wait(timeout=15)
                log.close()

if __name__ == '__main__':
    unittest.main()
