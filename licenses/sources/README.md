# Liberation Sans corresponding source

The four Liberation Sans TTFs in `static/vendor/pdfjs/standard_fonts/` are **unmodified official Liberation Fonts 1.07.4 release binaries**. On 2026-10-05, valtheum replaced PDF.js's differing copies with these official binaries to establish a documented match to the upstream source release. Font names and the 1.07.4 version are unchanged.

## Included source and verification

- [Complete upstream source archive](liberation-fonts-1.07.4.tar.gz) (unchanged): all SFD font sources, Makefile, scripts, README, AUTHORS, COPYING and License.txt.
- [Provenance and SHA-256 hashes](liberation-fonts-1.07.4.json): official source/binary release URLs, archive hashes and exact hashes of all four shipped fonts. Previous PDF.js hashes are recorded separately.
- [Upstream project](https://github.com/liberationfonts/liberation-1.7-fonts) and [official releases](https://releases.pagure.org/liberation-fonts/).

Run `node tools/licenses/verify-fonts.cjs` to check the packaged source archive and shipped fonts. This checks release provenance and file integrity, not bit-for-bit reproducibility of a new FontForge build.

## Build or modify the fonts

Extract the source archive with `tar -xzf liberation-fonts-1.07.4.tar.gz`. With FontForge and GNU Make installed, enter `liberation-fonts-1.07.4` and run `make`, as described in the original README. See the original Makefile for its export targets. The font sources are in `src/*.sfd` and build scripts in `scripts/`. FontForge is not bundled or required to run valtheum.

The fonts retain GPL v2 with the exceptions in the original License.txt; they are not MIT-licensed. If you modify them, follow the original modification and trademark/renaming terms. The archive preserves these terms and the full GPL text. Rebuilding valtheum from its source embeds the files under `static/vendor/pdfjs/standard_fonts/`.

## Redistributing valtheum

Use `scripts/package.ps1` to build a Windows distribution that includes this complete source archive and notices alongside the executables. Preserve this directory in both source and binary distributions. A standalone EXE with only a license notice is not the distribution method used here. When publishing downloadable binaries, provide the corresponding source alongside them; the generated distribution ZIP already does so.
