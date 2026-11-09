import re, sys

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
text = re.sub(r'\("(g\d{3})"\s*,\s*cpu_node\(', lambda m: 'cpu_row("%s", cpu_node(' % m.group(1), text)
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
