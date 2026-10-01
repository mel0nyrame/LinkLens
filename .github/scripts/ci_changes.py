"""Classify changes conservatively: only known documentation paths skip Rust."""
import os
import subprocess
from pathlib import Path


def docs_only(path):
    return (path.endswith(".md") or path.startswith(("docs/", ".assets/"))
            or path in {"LICENSE", ".gitignore"})


def requires_tooling(paths):
    return any(not docs_only(path) for path in paths)


def requires_rust(paths, has_cargo=True):
    return has_cargo and requires_tooling(paths)


if __name__ == "__main__":
    base = os.environ.get("BASE_SHA", "")
    head = os.environ["HEAD_SHA"]
    available = bool(base) and set(base) != {"0"}
    if available:
        available = subprocess.run(["git", "cat-file", "-e", base + "^{commit}"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
        if not available:
            available = subprocess.run(["git", "fetch", "--no-tags", "--depth=1", "origin", base],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not available:
        command = ["git", "ls-files", "-z"]
    else:
        command = ["git", "diff", "--name-only", "--no-renames", "-z", base, head]
    paths = subprocess.check_output(command).decode().rstrip("\0").split("\0")
    rust = requires_rust(paths, Path("Cargo.toml").exists())
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"rust={str(rust).lower()}\n")
        output.write(f"tooling={str(requires_tooling(paths)).lower()}\n")
    print(f"Changed files: {len(paths)}; Rust checks: {rust}")
