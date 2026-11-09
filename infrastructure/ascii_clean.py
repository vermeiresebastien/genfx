import sys
path = sys.argv[1]
data = open(path, "rb").read()
text = data.decode("utf-8")
repl = {"\u2013": "-", "\u2014": "-", "\u00b2": "2", "\u00c2": "", "\u00e9": "e", "\u00fc": "u", "\u00f6": "o", "\u00df": "ss", "\u00b0": " deg", "\u2192": "->", "\u2190": "<-", "\u20ac": "EUR", "\u00a9": "(c)", "\ufffd": ""}
out = []
for ch in text:
    if ord(ch) > 0x7F:
        out.append(repl.get(ch, "?"))
    else:
        out.append(ch)
open(path, "w", encoding="utf-8", newline="").write("".join(out))
print("done")
