import re, glob

files = glob.glob(r"D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\crates\catalog\src\*.rs")
# Drop the 'u' suffix on decimal integer literals used in comparisons so they
# match i32-valued accessors. Match `<op> <digits>u` with a following non-ident.
ops = ['==', '!=', '>=', '<=', '>', '<']
for f in files:
    t = open(f, encoding="utf-8").read()
    orig = t
    for op in ops:
        pat = re.compile(r'(' + re.escape(op) + r'\s*)(\d+)u\b')
        t = pat.sub(lambda m: m.group(1) + m.group(2), t)
    if t != orig:
        open(f, "w", encoding="utf-8", newline="").write(t)
print("done")
