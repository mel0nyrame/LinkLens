"""Validate an official release and pass its identity to both Rust commands."""
import os
import subprocess
from pathlib import Path

from check_release import validate


def build(root, tag, target):
    validate(tag, root)
    if target not in {"x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
                      "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"}:
        raise ValueError("Unsupported official Release target")
    env = {**os.environ, "LINKLENS_RELEASE_VERSION": tag, "LINKLENS_RELEASE_TARGET": target}
    subprocess.run(["cargo", "test", "--locked", "--target", target], cwd=root, env=env, check=True)
    subprocess.run(["cargo", "build", "--release", "--locked", "--bins", "--target", target],
                   cwd=root, env=env, check=True)


if __name__ == "__main__":
    build(Path.cwd(), os.environ["RELEASE_TAG"], os.environ["BUILD_TARGET"])
