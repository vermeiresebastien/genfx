import sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
import re
mods = ["effects_a", "effects_b", "effects_c", "families", "generators_a", "generators_b", "generators_c", "generators_d"]
for mod in mods:
    text = re.sub(r'(?<![\w:])' + mod + r'::', 'crate::' + mod + '::', text)
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
