"""Check project Markdown local links and conflict markers without network access."""
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

root = Path(__file__).resolve().parents[2]
tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode().split("\0")
# Internal skill packages and the historical local tracker have separate owners.
files = [root / name for name in tracked if name.endswith(".md")
         and not name.startswith((".agents/", ".scratch/"))]
errors = []
for path in files:
    if not path.exists():
        continue
    text = path.read_text(encoding="utf-8")
    if re.search(r"^(?:<{7}|={7}|>{7})(?: |$)", text, re.M):
        errors.append(f"{path.relative_to(root)}: unresolved conflict marker")
    # Ignore examples inside code fences and inline code.
    content = re.sub(r"```.*?```", "", text, flags=re.S)
    content = re.sub(r"`[^`\n]+`", "", content)
    links = re.findall(r'\]\(([^)]+)\)|(?:src|href)=[\"\']([^\"\']+)', content)
    for markdown, html in links:
        target = (markdown or html).strip().split(' "', 1)[0].strip("<>")
        parsed = urlsplit(target)
        if not target or parsed.scheme or target.startswith(("#", "//")):
            continue
        file_part = unquote(parsed.path)
        if not file_part:
            continue
        dest = (root / file_part.lstrip("/")) if file_part.startswith("/") else path.parent / file_part
        if not dest.exists():
            errors.append(f"{path.relative_to(root)}: missing local link {target}")
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"Documentation checks passed ({len(files)} Markdown files).")
