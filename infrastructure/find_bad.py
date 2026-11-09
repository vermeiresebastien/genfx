import sys

data = open(sys.argv[1], "rb").read()
try:
    text = data.decode("utf-8")
    print("valid utf-8")
except UnicodeDecodeError as e:
    print("invalid at", e.start, data[max(0, e.start - 60):e.start + 60])
    raise SystemExit(0)
for idx, ch in enumerate(text):
    if ord(ch) > 0x7F and ch not in "–—éüöß→←°€©":
        line = text[:idx].count("\n") + 1
        print("nonascii", repr(ch), "line", line, text[max(0, idx - 50):idx + 20].replace("\n", " "))
