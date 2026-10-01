"""Exercise CI routing, release gates, archives and installer against local fixtures."""
import hashlib
import io
import os
import subprocess
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from ci_changes import requires_rust, requires_tooling
from check_release import validate
from package_release import package

ROOT = Path(__file__).resolve().parents[2]


class ReleaseTools(unittest.TestCase):
    def test_ci_routes_changes(self):
        for paths in (["README.md"], ["docs/guide.md", ".assets/hero.png"], ["LICENSE"]):
            self.assertFalse(requires_rust(paths))
            self.assertFalse(requires_tooling(paths))
        for paths in (["src/app.rs"], ["Cargo.lock"], ["install.sh"], [".github/workflows/ci.yml"]):
            self.assertTrue(requires_rust(paths))
            self.assertTrue(requires_tooling(paths))
        self.assertFalse(requires_rust(["install.sh"], has_cargo=False))
        self.assertTrue(requires_tooling(["install.sh"]))
        self.assertTrue(requires_rust(["README.md", "tests/regression.rs"]))

    def test_release_requires_matching_version_and_notes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nversion = "1.2.3"\n')
            notes = root / ".github/releases/v1.2.3.md"
            notes.parent.mkdir(parents=True)
            with self.assertRaises(FileNotFoundError):
                validate("v1.2.3", root)
            notes.write_text("TODO " + "placeholder " * 20)
            with self.assertRaises(ValueError):
                validate("v1.2.3", root)
            notes.write_text("A complete hand-written release note. " * 5)
            self.assertEqual(validate("v1.2.3", root), notes)
            for tag in ("v1.2.4", "bad/tag", "v1.2.3-beta"):
                with self.assertRaises(ValueError):
                    validate(tag, root)

    def test_archive_contents_and_checksum(self):
        for target in ("x86_64-unknown-linux-musl", "x86_64-pc-windows-msvc"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binaries = root / "target" / target / "release"
                binaries.mkdir(parents=True)
                suffix = ".exe" if "windows" in target else ""
                expected = {"LICENSE", "README.md", "linklens" + suffix, "llens" + suffix}
                for name in expected:
                    parent = root if name in {"LICENSE", "README.md"} else binaries
                    (parent / name).write_bytes(b"fixture")
                archive = package(root, "v1.2.3", target)
                if suffix:
                    with zipfile.ZipFile(archive) as contents:
                        names = set(contents.namelist())
                else:
                    with tarfile.open(archive) as contents:
                        names = set(contents.getnames())
                self.assertEqual(names, expected)
                self.assertNotIn(b"\r", archive.with_name(archive.name + ".sha256").read_bytes())
                sums = archive.with_name(archive.name + ".sha256").read_text()
                self.assertEqual(sums.split()[0], hashlib.sha256(archive.read_bytes()).hexdigest())

    def run_installer(self, os_name, arch, corrupt=False, missing=False, latest=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            mocks = root / "mocks"
            mocks.mkdir()
            target_os = "apple-darwin" if os_name == "Darwin" else "unknown-linux-musl"
            cpu = "aarch64" if arch in {"arm64", "aarch64"} else "x86_64"
            windows = os_name.startswith("MINGW")
            target = "x86_64-pc-windows-msvc" if windows else f"{cpu}-{target_os}"
            suffix = ".exe" if windows else ""
            asset = f"linklens-v1.2.3-{target}{'.zip' if windows else '.tar.gz'}"
            archive = root / asset
            names = ["linklens" + suffix] + ([] if missing else ["llens" + suffix])
            if windows:
                with zipfile.ZipFile(archive, "w") as output:
                    for name in names:
                        output.writestr(name, b"fixture executable")
            else:
                with tarfile.open(archive, "w:gz") as output:
                    for name in names:
                        data = b"fixture executable"
                        info = tarfile.TarInfo(name)
                        info.size = len(data)
                        info.mode = 0o755
                        output.addfile(info, io.BytesIO(data))
            digest = "0" * 64 if corrupt else hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / "SHA256SUMS").write_text(f"{digest}  {asset}\n")
            (mocks / "uname").write_text(f'#!/bin/sh\nif [ "$1" = -s ]; then echo "{os_name}"; else echo "{arch}"; fi\n')
            # Model only the downloader, leaving extraction, hashing and installation real.
            (mocks / "curl").write_text('''#!/usr/bin/env python3
import os, sys, shutil
from pathlib import Path
args = sys.argv[1:]
if '-w' in args:
    print('https://github.com/mel0nyrame/LinkLens/releases/tag/v1.2.3', end='')
else:
    url = next(arg for arg in args if arg.startswith('https://'))
    output = args[args.index('-o') + 1]
    shutil.copyfile(Path(os.environ['FIXTURE_DIR']) / url.rsplit('/', 1)[1], output)
''')
            for mock in mocks.iterdir():
                mock.chmod(0o755)
            install_dir = root / "install space"
            env = {**os.environ, "PATH": str(mocks) + os.pathsep + os.environ["PATH"],
                   "FIXTURE_DIR": str(root), "LINKLENS_INSTALL_DIR": str(install_dir),
                   "LINKLENS_VERSION": "" if latest else "v1.2.3"}
            result = subprocess.run(["sh", str(ROOT / "install.sh")], env=env, capture_output=True, text=True, errors="replace")
            should_fail = corrupt or missing or os_name == "FreeBSD"
            if should_fail:
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse((install_dir / ("linklens" + suffix)).exists())
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                for name in ("linklens", "llens"):
                    self.assertEqual((install_dir / (name + suffix)).read_bytes(), b"fixture executable")

    def test_install_system_selection(self):
        for os_name, arch in (("Darwin", "arm64"), ("Darwin", "x86_64"),
                              ("Linux", "aarch64"), ("Linux", "x86_64"), ("MINGW64_NT", "x86_64")):
            with self.subTest(os=os_name, arch=arch):
                self.run_installer(os_name, arch)

    def test_install_latest_redirect(self):
        self.run_installer("Darwin", "arm64", latest=True)

    def test_install_fails_before_changing_destination(self):
        self.run_installer("Linux", "x86_64", corrupt=True)
        self.run_installer("Linux", "x86_64", missing=True)
        self.run_installer("FreeBSD", "x86_64")


if __name__ == "__main__":
    unittest.main()
