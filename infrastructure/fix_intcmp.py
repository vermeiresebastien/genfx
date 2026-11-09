import re, glob

# Since int/enum/bool accessors now return i32, normalise shader comparisons:
#  - "p_xxx() == 3u"  -> "p_xxx() == 3"
#  - "p_xxx() != 2u"  -> "p_xxx() != 2"
#  - comparisons to u32 constants like `0u`, `1u` on the RHS of accessor calls
# Bool accessors used as `if (p_x())` or `!p_x()` or `p_x() && ...` must become
# truthy comparisons. We handle known bool ids by scanning manifests? Hard.
# Instead handle the general cases that are unambiguous.

files = glob.glob(r"D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\crates\catalog\src\*.rs")
for f in files:
    t = open(f, encoding="utf-8").read()
    orig = t
    # accessor compared to u32 literal
    t = re.sub(r'(p_[A-Za-z0-9_]+\(\))\s*(==|!=)\s*(\d+)u\b', r'\1 \2 \3', t)
    # select(false/true, ..., cond) where cond uses accessor: handled later.
    if t != orig:
        open(f, "w", encoding="utf-8", newline="").write(t)
print("done")
