# Spelling dictionaries

Pure spell-checks documents with [`spellbook`], a pure-Rust checker that reads
Hunspell's `.aff`/`.dic` dictionary pair.

## What's bundled

`en_US/` holds the SCOWL-derived American English dictionary (version
2020.12.07, taken from [`wooorm/dictionaries`]), so `F7` works out of the box
with no download and no system dependency. Its terms — permissive, redistribution
allowed with the notices intact — are in `en_US/LICENSE`.

The files are embedded into the binary at build time behind the default
`bundled-dictionary` feature. Build with `--no-default-features` to leave them
out; spell checking then needs one of the dictionaries below.

## Adding another language

Drop a Hunspell dictionary pair into `~/.config/pure/dictionaries/` and name it
after its language tag:

```
~/.config/pure/dictionaries/de_DE.aff
~/.config/pure/dictionaries/de_DE.dic
```

Then point Pure at it in `~/.config/pure/config.toml`:

```toml
spell_language = "de_DE"
```

Pure also looks in `$DICPATH` and the usual system directories
(`/usr/share/hunspell`, `/usr/share/myspell`, `/opt/homebrew/share/hunspell`,
`~/Library/Spelling`, …), so a dictionary installed by your package manager is
picked up as well. `spell_dictionary` in the config file overrides the search
with an explicit path to a `.dic` file.

[`spellbook`]: https://github.com/helix-editor/spellbook
[`wooorm/dictionaries`]: https://github.com/wooorm/dictionaries
