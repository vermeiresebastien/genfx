import re, sys

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
# find calls crate::module::NAME() occurring as array elements (they return 3-tuples)
for call in set(re.findall(r'crate::\w+::(\w+)\(\)', text)):
    text = re.sub(r'crate::\w+::' + call + r'\(\)', 'crate::%s' % call, text)
# fix push signature: shader key is the manifest's shader file / type key; use manifest id as key
text = text.replace('let mut push = |shader: &\'static str, m: Manifest, body: &\'static str| {\n        let path = m.implementation.path.clone();\n        let _ = shader;',
                    "let mut push = |_key: &'static str, m: Manifest, body: &'static str| {\n        let path = m.implementation.path.clone();")
open(path, "w", encoding="utf-8", newline="").write(text)
print("done")
