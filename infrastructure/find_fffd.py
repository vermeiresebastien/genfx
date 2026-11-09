import sys

data = open(sys.argv[1], "rb").read()
text = data.decode("utf-8", errors="replace")
for idx, ch in enumerate(text):
    if ch == "\ufffd":
        line = text[:idx].count("\n") + 1
        print("replacement at line", line, "byte", idx)
