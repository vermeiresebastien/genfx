import io, re, sys

path = sys.argv[1]
lines = open(path, encoding="utf-8").read().split("\n")
out = []
skip = set()
for idx, ln in enumerate(lines):
    if ln.strip().startswith("'" ) and "=>" in ln:
        literal = ln.strip().split("=>")[0].strip()
        body = literal[1:-1]
        if len(body) != 1:
            skip.add(idx)
            continue
    out.append(ln)
open(path, "w", encoding="utf-8", newline="").write("\n".join(out))
print("removed", len(skip))
