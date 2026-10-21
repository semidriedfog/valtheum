# Bundled dependency patches

## PDF.js 6.3.289 — page error propagation

`static/vendor/pdfjs/pdf.mjs`, `PDFPageProxy._pumpOperatorList`: reject pending
operator-list/display promises and cancel render tasks before completing a failed
operator list. Otherwise an oversized image error can finish the page successfully
with its image missing, even with `stopAtErrors: true`. Remove synthetic
operator-list tasks from the task set so cleanup can finish. Evict the failed
intent state before cancelling tasks, then request cleanup: the display-ready
promise may already have resolved, and keeping it lets later retries accept the
truncated list as a successful page. A retry must request a new operator list.

Keep this change when updating PDF.js unless the replacement version passes
`node --test tests/pdf-engine.test.cjs` without it. The test uses a generated
4096 × 4097 image to check image preservation and explicit size-limit errors,
including repeated renders on the same page proxy and a zoom change after failure.
Viewer behavior and progress-save failures are also covered in `tests/frontend.test.cjs`.

## Liberation Sans 1.07.4 — official release binaries (2026-10-05)

Replaced all four `static/vendor/pdfjs/standard_fonts/LiberationSans-*.ttf` files with the unmodified official 1.07.4 TTF release. The earlier PDF.js files had the same version metadata but differed in font tables. The official source archive, build scripts, hashes, and distribution instructions are in [licenses/sources](../licenses/sources/README.md). The upstream GPL v2 license and exceptions remain unchanged.

The PDF.js JavaScript file now carries a prominent valtheum modification notice before its original license header.

## Liberation Sans 1.07.4 — official release binaries (2026-10-05)

Replaced all four `static/vendor/pdfjs/standard_fonts/LiberationSans-*.ttf` files with the unmodified official 1.07.4 TTF release. The earlier PDF.js files had the same version metadata but differed in font tables. The official source archive, build scripts, hashes, and distribution instructions are in [licenses/sources](../licenses/sources/README.md). The upstream GPL v2 license and exceptions remain unchanged.

The PDF.js JavaScript file now carries a prominent valtheum modification notice before its original license header.
