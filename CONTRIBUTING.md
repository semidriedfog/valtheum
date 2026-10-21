# Contributing to valtheum

The project targets Windows x64 with Rust, SQLCipher and WebView2. Start with the [build guide](docs/BUILD.md); the [English README](README.en.md) and [Japanese README](README.ja.md) describe supported features.

## Changes and verification

- Describe the problem, expected behavior and steps to reproduce it using synthetic files.
- Keep changes focused. Add regression coverage for bugs involving stored data, asynchronous requests or library switching.
- Use the checked-in Cargo lockfiles. After fetching dependencies with `cargo fetch --locked`, run `scripts/build.ps1 -Test`, or the individual checks in the build guide.
- For UI strings, update `tools/i18n/extra.json` for English and Japanese and regenerate the catalogs with `node tools/i18n/build.cjs`. Keep the three READMEs consistent when user-facing behavior changes.
- Preserve old encrypted-file compatibility. Format changes need documented compatibility decisions and synthetic restore tests; see [VTA_FORMAT.md](docs/VTA_FORMAT.md).
- Keep upstream notices, source bundles and public test vectors intact. Changes to bundled PDF.js belong in [VENDOR_PATCHES.md](docs/VENDOR_PATCHES.md). Update the license inventory when dependencies change.

## Issues and pull requests

Include the Windows version, app build, affected media type and whether storage is local or on a NAS. State which checks you ran and which environments you could not verify. Never attach a personal library, database, key file, backup, password or unredacted log; use a minimal synthetic reproduction instead. The key in `tests/fixtures/vta1/` is intentionally public and must never protect real data.

If a security issue could expose private data, use the repository's private vulnerability-reporting option when available. Do not publish sensitive reproduction data in a public issue.

The project's original code is [MIT-licensed](LICENSE). Third-party components retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
