import sys
import re

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
fixed = re.sub(r"'static str>", "'static str)", text)
open(path, "w", encoding="utf-8", newline="").write(fixed)
print("fixed", text.count("'static str>"))
