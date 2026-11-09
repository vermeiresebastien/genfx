import re, sys

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
# Replace v.push(("gNNN", with v.push(scene_row("gNNN",
text = re.sub(r'v\.push\(\("(g\d{3})"\s*,\s*', lambda m: 'v.push(scene_row("%s", ' % m.group(1), text)
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
