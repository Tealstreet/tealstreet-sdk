# Pinned Unihan root data

Generated using `icu4x-datagen 2.3.0`, CLDR 48.2.1, ICU export release-78.1rc,
explicit `--collation-root-han unihan --locales und`. Node ICU 78 uses Unihan
radical-stroke order; ICU4X's default baked root uses implicit Han order instead.
`profile.json` records the generated provider's SHA-256. The live oracle asserts
its version fields, complete file list, and checksums. The crate-version test
also checks the actual resolved data package metadata.

Reproduce using `tests/generate-collation-data.py PATH_TO_ICU4X_DATAGEN` with the
pinned binary installed into an isolated target directory (two cargo jobs). The
generator is not a runtime or workspace build dependency. Set RAYON_NUM_THREADS=2.
The script expands provider macro arms into checked-in declarations and limits
associated constants to crate visibility;
no semantic data changes are made. Keep the Unicode license with these files.

Keep all seven collation tables together. Jamo entries reference offsets in the
root table; replacing only the root with Unihan data while retaining implicit-Han
auxiliary tables produced a debug overflow on U+117C U+3C321. Normalization data
remains pinned independently to icu_normalizer_data 2.3.0.

The generator includes metadata/tailoring constants beyond root even with `und`.
They are private; only the explicit root constructor is exposed. Initialization
uses a fallible provider constructor, never an environment locale or byte fallback.
