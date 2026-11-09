import sys
s = open(sys.argv[1], encoding="utf-8").read()
lines = s.split("\n")
mode = None
line = 1
stack = []
i = 0
while i < len(s):
    ch = s[i]
    if mode is None:
        if ch == 'r' and i + 1 < len(s) and s[i+1] == '#':
            end = s.find('"#', i)
            if end != -1:
                line += s[i:end].count('\n'); i = end + 2; continue
        if s.startswith('//', i):
            end = s.find('\n', i); i = end if end != -1 else len(s); continue
        if ch == '"':
            end = i + 1
            while end < len(s) and s[end] != '"':
                if s[end] == '\\': end += 1
                end += 1
            line += s[i:end].count('\n')
            i = end + 1
            continue
        if ch == "'":
            # char literal: 'x' or '\x'
            if i + 2 < len(s) and s[i+1] == '\\':
                end = s.find("'", i + 3)
            else:
                end = s.find("'", i + 1)
            i = end + 1 if end != -1 else i + 1
            continue
        if ch in '{([':
            stack.append((ch, line, i))
        elif ch in '})]':
            if stack: stack.pop()
            else: print("extra", ch, line)
        if ch == '\n': line += 1
        i += 1
        continue
print("unclosed:", [(c, l) for c, l, _ in stack])
if stack:
    c, l, pos = stack[0]
    print("at line", l, repr(s[max(0,pos-120):pos+120]))
