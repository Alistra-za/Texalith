"""Copy the include closure of the two shaders render3d compiles for built-in forms.

Usage: python -I tools/make_cs_bundle.py <CS Shaders folder> <dest shaders folder>

Follows includes the way render3d's includes.rs does: every #include in a file, whatever #ifdef it sits under,
looked up beside the including file and then at the Shaders root. Files are copied byte for byte: they must stay
unmodified (see assets/cs-shaders/PIN.toml). Exits 1 on any include it cannot find.
"""
import os
import re
import shutil
import sys

ENTRIES = ["Lighting.hlsl", "DynamicCubemaps/SpecularIrradianceCS.hlsl"]
INCLUDE = re.compile(r'^\s*#\s*include\s*["<]([^">]+)[">]', re.M)


def closure(root):
    seen, missing = {}, []

    def walk(path):
        key = os.path.normcase(os.path.normpath(path))
        if key in seen:
            return
        seen[key] = os.path.relpath(path, root)
        with open(path, encoding="utf-8", errors="replace") as f:
            text = f.read()
        for name in INCLUDE.findall(text):
            beside, at_root = os.path.join(os.path.dirname(path), name), os.path.join(root, name)
            found = beside if os.path.isfile(beside) else at_root if os.path.isfile(at_root) else None
            if found:
                walk(found)
            else:
                missing.append((seen[key], name))

    for entry in ENTRIES:
        walk(os.path.join(root, entry))
    return sorted(seen.values()), missing


def main():
    root, dest = sys.argv[1], sys.argv[2]
    files, missing = closure(root)
    if missing:
        print("missing includes:", missing)
        sys.exit(1)
    total = 0
    for rel in files:
        target = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copyfile(os.path.join(root, rel), target)
        total += os.path.getsize(target)
    print(f"{len(files)} files, {total} bytes")


if __name__ == "__main__":
    main()
