import sys
sys.path.insert(0, r"D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX")
text = open(sys.argv[1], encoding="utf-8").read()
# temporarily print duplicate IDs by inspecting source ids
import re
ids = re.findall(r'"(fx\.[a-z_]+|gen\.[a-z_]+|flux\.[a-z_]+|preset\.[a-z_]+)"', text)
from collections import Counter
print([k for k, v in Counter(ids).items() if v > 1])
