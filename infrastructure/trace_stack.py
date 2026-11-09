import sys

path = sys.argv[1]
lines = open(path, encoding="utf-8").read().split("\n")
out = []
mode = None
depth = 0
stack = []
line_no = 0
for ln in lines:
    line_no += 1
    i = 0
    while i < len(ln):
        ch = ln[i]
        if mode is None:
            if ch == 'r' and i + 1 < len(ln) and ln[i+1] == '#':
                out.append(ln[i:]); i = len(ln); break
            if ln.startswith('//', i):
                out.append(ln[i:]); i = len(ln); break
            if ch == '"':
                mode = 'str'; out.append(ch); i += 1; continue
            if ch == "'":
                mode = 'char'; out.append(ch); i += 1; continue
            if ch in '{([':
                stack.append((ch, line_no, depth))
                depth += 1
            elif ch in '})]':
                depth -= 1
                if stack:
                    stack.pop()
                else:
                    print("extra close", ch, "line", line_no)
            out.append(ch); i += 1; continue
        if ch == '\\':
            out.append(ln[i:i+2]); i += 2; continue
        if (mode == 'str' and ch == '"') or (mode == 'char' and ch == "'"):
            mode = None; out.append(ch); i += 1; continue
        out.append(ch); i += 1
    for c, l, d in stack:
        if d > 0:
            print("depth", d, "open", c, "line", l)
            break
    if stack:
        print(f"L{line_no}:{stack}")
