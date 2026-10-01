"""Package the native binaries, license and README with a SHA-256 checksum."""
import hashlib
import os
import tarfile
import zipfile
from pathlib import Path


def package(root, tag, target):
    windows = "windows" in target
    suffix = ".exe" if windows else ""
    binaries = root / "target" / target / "release"
    inputs = [(binaries / (name + suffix), name + suffix) for name in ("linklens", "llens")]
    inputs += [(root / name, name) for name in ("LICENSE", "README.md")]
    for source, _ in inputs:
        if not source.is_file():
            raise FileNotFoundError(source)
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    archive = dist / f"linklens-{tag}-{target}{'.zip' if windows else '.tar.gz'}"
    if windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for source, name in inputs:
                output.write(source, name)
    else:
        with tarfile.open(archive, "w:gz") as output:
            for source, name in inputs:
                output.add(source, arcname=name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n")
    return archive


if __name__ == "__main__":
    print(package(Path.cwd(), os.environ["RELEASE_TAG"], os.environ["BUILD_TARGET"]))
