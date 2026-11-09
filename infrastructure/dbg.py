import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
fn_names = re.findall(r'^\s*crate::\w+::(\w+?)\(\),?$', text, re.M)
print("calls found:", len(fn_names), fn_names[:5])
fn_names += re.findall(r'^\s*crate::\w+::(\w+?)    ,$', text, re.M)
print("all:", len(fn_names))
