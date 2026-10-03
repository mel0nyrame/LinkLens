"""Require a semver tag, matching Cargo version and explicit release prose."""
import os
import re
import tomllib
from pathlib import Path


def validate(tag, root):
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        raise ValueError("Release tag must use vMAJOR.MINOR.PATCH")
    with (root / "Cargo.toml").open("rb") as source:
        version = tomllib.load(source)["package"]["version"]
    if tag != f"v{version}":
        raise ValueError("Tag and Cargo version must match")
    notes = root / ".github" / "releases" / f"{tag}.md"
    text = notes.read_text(encoding="utf-8")
    if len(text.strip()) < 100 or re.search(r"TODO|TBD|待补充", text):
        raise ValueError("Write complete release notes before tagging")
    return notes


if __name__ == "__main__":
    print(validate(os.environ["RELEASE_TAG"], Path.cwd()))
