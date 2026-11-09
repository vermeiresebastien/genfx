import re, glob, sys

# WGSL uses the '%' operator, not a mod() function. Replace mod(a, b) calls with
# a helper function that we add to the shared preamble. Simplest robust approach:
# define `mod` as a function in wgsl_common via the preamble so existing code
# keeps working, and WGSL allows user functions named mod? No — the earlier error
# was "no definition in scope for identifier mod", meaning mod() is legal as a
# user function. So define it in the preamble.
