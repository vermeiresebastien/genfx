# infrastructure/

Scratch tooling used while building Flux. It is not a library and nothing in
`crates/` or `apps/` imports it. Every file below was classified by reading it:
whether it writes, whether it takes a path argument, and whether it still
carries a pre-relocation absolute path. The classification matters, because one
group would damage the tree if it were re-run.

`PROGRESS.md` is the build ledger for this folder: the host specification, the
T00 findings on FFmpeg, Vulkan, NDI and signing, and a pointer to
`implementation/STATE.json` for the current task.

## Ongoing tools

* `verify.ps1` — the clean-checkout verification suite. Run it from the
  repository root. It writes aggregate results to
  `implementation/evidence/verify.json` and reports GPU and media steps as
  `unavailable` rather than failing when the hardware is absent.
* `make_icon.py` — generates a 32-bit BGRA `.ico` from scratch.

## Read-only inspectors

These print findings and change nothing. Each takes the file or directory to
inspect as `sys.argv[1]`.

* `bracket_check.py`, `trace_stack.py` — structural checks over source.
* `find_bad.py`, `find_fffd.py` — locate mojibake and U+FFFD replacement
  characters left by the encoding port.
* `dbg.py` — small debug dump.
* `find_dupes.py` — prints duplicate effect IDs in the file you name. It still
  inserts the old project root into `sys.path`, but it imports nothing from
  there, so that line is inert.

`fdupes.py` is also read-only, but it is bound to a hardcoded glob under the old
project root rather than to an argument. It runs without error and prints
nothing, because the glob no longer matches any file.

## Rewrites the single file you name

Each takes one path as `sys.argv[1]` and rewrites it in place. They cannot touch
anything you did not point them at, but there is no dry-run mode and no backup.

* `flatten_utility.py` — rewrites `crate::module::NAME()` call sites and one
  `push` closure signature.
* `ascii_clean.py` — strips every non-ASCII character, mapping a fixed table of
  accented and punctuation characters and replacing anything unmapped with `?`.
  Irreversible.
* `add_parens.py`, `drop_multichar.py`, `fix_encoding.py`, `fix_typo.py`,
  `requalify.py`, `restore_calls.py`, `wrap_cpu.py`, `wrap_scene.py` — the same
  shape, each performing one narrow edit from the port.

## Do not re-run: spent migrations over the whole crate

These three glob `crates/catalog/src/*.rs` by hardcoded absolute path and rewrite
every match in place. They are not idempotent, and the migrations they performed
are already part of the current tree.

* `rename_mod.py` — renamed `mod(` / `modv(` to `fmod(` / `fmodv(`, because
  `mod` is a Rust keyword.
* `fix_intcmp.py`, `fix_ucomp.py` — normalised shader comparisons after the
  int/enum/bool accessors changed to return `i32`.

They are the only genuinely dangerous files here, and the reason is specific: if
someone "helpfully" repaired their dead path, the next run would rewrite working
source across the whole catalog crate. They are kept as the record of how the
port was done, not as tools.

## Stubs that do nothing

* `fix_wgsl.py` — imports and an eight-line comment reasoning about whether WGSL
  permits a user-defined `mod` function. The conclusion was reached in the
  comment; no code was written.
* `noop.py` — two lines that insert the old project root into `sys.path`, then
  exit.

## Hardcoded paths in this folder are historical

Six files contain absolute paths of the form
`D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\...`: `fdupes.py`, `find_dupes.py`,
`fix_intcmp.py`, `fix_ucomp.py`, `noop.py` and `rename_mod.py`. That was this
repository's location before every project on the machine was relocated to
`D:\Projects` on 2026-11-13; the equivalent path now is
`D:\Projects\skeer-audio\genfx`.

Those paths were deliberately **not** updated. For the three spent migrations the
dead path is now the only thing standing between them and a destructive rewrite
of the catalog crate. For `fdupes.py`, `find_dupes.py` and `noop.py` the line is
inert. No file in the ongoing-tools, read-only or single-file groups carries one:
those take their target as an argument or resolve relative to the repository
root.
