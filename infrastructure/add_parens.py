import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
# Add () to bare crate::mod::fn_name references in array/list positions, except row functions that return Vec.
text = re.sub(r'crate::(effects_a|effects_b|effects_c|families|generators_a|generators_b|generators_c)::(\w+)(?=[,\n])',
              lambda m: 'crate::%s::%s()' % (m.group(1), m.group(2)), text)
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
