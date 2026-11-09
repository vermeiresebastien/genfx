import re, glob
from collections import Counter
allids = []

def scan_file(f):
    t = open(f, encoding="utf-8").read()
    for m in re.finditer(r'effect\(\s*\n?\s*"([a-z0-9_]+\.?[a-z0-9_]*)"', t):
        allids.append((m.group(1), f.split("\\")[-1]))

for f in glob.glob(r"D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\crates\catalog\src\*.rs"):
    scan_file(f)

c = Counter(i for i, _ in allids)
for k, v in c.items():
    if v > 1:
        print(k, v, [f for i, f in allids if i == k])
