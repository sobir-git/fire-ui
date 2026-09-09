#!/usr/bin/env python3
"""Bound local Cargo caches; clean only native dev/test/release build products."""
import argparse
from contextlib import ExitStack
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from probe_lifetime import install_signal_cleanup, run_owned

ROOT = Path(__file__).resolve().parents[1]
GIB = 1024 ** 3
MAX_TARGET = 4 * GIB
MIN_FREE = 8 * GIB


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def worktrees():
    return [Path(record.split('\n', 1)[0].removeprefix('worktree ')).resolve()
            for record in git(ROOT, 'worktree', 'list', '--porcelain').split('\n\n')]


def validate_target(root, target):
    root = root.resolve(strict=True)
    target = Path(os.path.abspath(target))
    if target != root / 'target' or target.resolve() != target:
        raise ValueError(f'Refusing non-local or symlinked target: {target}')
    if not target.exists():
        return target
    if not target.is_dir():
        raise ValueError(f'Not a directory: {target}')
    # Cargo creates this record. An arbitrary directory named target is not enough.
    marker = target / '.rustc_info.json'
    if marker.is_symlink() or not marker.is_file():
        raise ValueError(f'No Cargo fingerprint in {target}; refusing ambiguous cleanup')
    if not isinstance(json.loads(marker.read_text()).get('rustc_fingerprint'), int):
        raise ValueError(f'Invalid Cargo fingerprint in {target}')
    for name in ('debug', 'release'):
        profile = target / name
        if profile.is_symlink() or (profile.exists() and not profile.is_dir()):
            raise ValueError(f'Refusing ambiguous profile: {profile}')
        lock = profile / '.cargo-lock'
        if profile.exists() and (not lock.is_file() or lock.is_symlink()):
            raise ValueError(f'Missing or symlinked Cargo lock: {lock}')
    if git(root, 'ls-files', '--', 'target'):
        raise ValueError(f'Tracked files in {target}; refusing cleanup')
    return target


def size(directory):
    # Count allocated bytes once per inode, including Cargo's hardlinked binaries.
    seen = set()
    total = 0
    for base, _, files in os.walk(directory, followlinks=False):
        for name in files:
            try:
                stat = (Path(base) / name).lstat()
            except FileNotFoundError:
                continue  # Another worktree may currently be building.
            key = (stat.st_dev, stat.st_ino)
            if key not in seen:
                total += stat.st_blocks * 512
                seen.add(key)
    return total


def preflight(roots, max_target=MAX_TARGET, min_free=MIN_FREE):
    for root in roots:
        if not root.is_dir():
            continue
        target = root / 'target'
        if target.resolve() != target:
            raise ValueError(f'Refusing shared/symlinked target: {target}')
        used = size(target)
        free = shutil.disk_usage(root).free
        print(f'{target}: {used / GIB:.2f} GiB; free {free / GIB:.2f} GiB', flush=True)
        if used >= max_target or free < min_free:
            raise ValueError(f'Storage budget exceeded at {root}. Run '
                             f'python3 tools/build_storage.py --worktree {root} clean')


def clean(root):
    target = validate_target(root, root / 'target')
    if not target.exists():
        return
    # Same flock used by Cargo on Unix. Keep lock inodes in place so a queued
    # Cargo process cannot acquire an unlinked lock and race another build.
    with ExitStack() as stack:
        profiles = [target / name for name in ('debug', 'release') if (target / name).exists()]
        for profile in profiles:
            lock = stack.enter_context((profile / '.cargo-lock').open('r+'))
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise ValueError(f'Active Cargo build in {profile}; retry after it exits') from error
        validate_target(root, target)
        for profile in profiles:
            for child in profile.iterdir():
                if child.name == '.cargo-lock':
                    continue
                if child.is_dir() and not child.is_symlink():
                    shutil.rmtree(child)
                else:
                    child.unlink()
    print(f'Cleaned native dev/test/release products in {target}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worktree', type=Path, default=ROOT)
    parser.add_argument('action', choices=('check', 'clean', 'cargo'))
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    roots = worktrees()
    root = args.worktree.resolve(strict=True)
    if root not in roots:
        parser.error('Worktree must be registered in this Fire UI repository')
    try:
        if args.action == 'clean':
            if args.arguments:
                parser.error('clean accepts no paths or Cargo options')
            clean(root)
        else:
            preflight(roots)
            if args.action == 'cargo':
                # Verify Cargo configuration, including environment redirects.
                metadata = json.loads(subprocess.check_output(
                    ['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=root))
                if Path(metadata['target_directory']).resolve() != root / 'target':
                    raise ValueError('Guarded Cargo requires the worktree-local target directory')
                if not args.arguments or args.arguments[0] not in ('build', 'test', 'check', 'clippy', 'doc', 'run', 'package'):
                    raise ValueError('Expected build/test/check/clippy/doc/run/package')
                if any(arg.startswith(('--target-dir', '--manifest-path', '--config')) for arg in args.arguments):
                    raise ValueError('Target/manifest/config overrides bypass the storage guard')
                result = run_owned(['cargo', *args.arguments], cwd=root)
                if result.returncode:
                    return result.returncode
                preflight(roots)
            elif args.arguments:
                parser.error('check accepts no arguments')
    except (ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    install_signal_cleanup()
    raise SystemExit(main())
