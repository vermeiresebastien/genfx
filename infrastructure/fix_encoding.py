import sys

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
text = text.replace('256\u00c2\u00b2', '256 squared').replace('\u00c2\u00b2', ' squared ')
text = text.replace('256\u2013', '256-').replace('\u2013', '-')
text = text.replace('m/s\u00c2\u00b2', 'm/s^2').replace('m/s\u00b2', 'm/s^2')
text = text.replace('32\u00c2\u00b2', '32 squared').replace('32\u00b2', '32 squared')
open(path, "w", encoding="utf-8", newline="").write(text)
