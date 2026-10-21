# Third-party licenses and notices

valtheum's original code is licensed under the [MIT License](LICENSE). Third-party code, fonts, and other resources retain their own licenses and copyright notices. The project's MIT license does not replace those terms.

This document covers the Windows x64 app, standalone Rust recovery tools, and bundled PDF resources. Keep the notices and font source included in the generated distribution ZIP with binary distributions.

## Rust dependencies

The [distributed component inventory](licenses/DISTRIBUTION.md) lists the notices included in the Windows distribution and the in-app license viewer. Runtime dependencies are selected from normal dependency edges of the locked Windows Cargo graph. Direct procedural-macro notices are retained conservatively for generated app code; their host-only helpers are excluded. This inventory covers all three shipped executables, not a claim that every component is linked into each executable.

- Declared license expressions are retained from Cargo metadata, including multi-license expressions.
- License texts, copyright statements, and notices are copied without editing. Nested notices for bundled native code are included.
- [The distribution manifest](licenses/distribution.json) records component versions, selected notice paths, text hashes, and the files to package.
- Version-specific upstream notices are preserved when the corresponding crate archive omits its license texts.
- Benchmark-only dependencies, Python tools, and build/test-only helpers are excluded from the app catalog. Their inventories and provenance remain in the source repository's development documentation.

Regenerate the app catalog from a populated Cargo cache:

```powershell
node tools/licenses/bundle.cjs
```

The script uses `--locked --offline` Cargo metadata and does not download or execute upstream source code. If an updated dependency lacks inventory records, generation fails; refresh the full source inventory with `node tools/licenses/generate.cjs` first.

### Native libraries

The Rust wrapper's license is not a substitute for the wrapped library's license.

| Component | License / notice | Included text |
| --- | --- | --- |
| SQLCipher bundled by libsqlite3-sys 0.38.2 | BSD-3-Clause (Zetetic) | [SQLCipher LICENSE](licenses/rust/libsqlite3-sys-0.38.2/sqlcipher/LICENSE) |
| libsqlite3-sys wrapper | MIT | [Wrapper LICENSE](licenses/rust/libsqlite3-sys-0.38.2/LICENSE) |
| OpenSSL 3.6.3 | Apache-2.0 | [OpenSSL LICENSE](licenses/rust/openssl-src-300.6.1+3.6.3/openssl/LICENSE.txt) |

Other compression libraries and their nested notices are listed in the [distributed inventory](licenses/DISTRIBUTION.md). OpenSSL is included explicitly even though its sources are supplied by a build dependency. The local static OpenSSL build used for the release reports version 3.6.3; if you supply a different `OPENSSL_DIR`, update the version and notices for that actual installation as well.

SQLCipher is listed separately from its MIT-licensed Rust wrapper in the offline app catalog. Its original Zetetic copyright statement and BSD-3-Clause text are preserved without replacing their dates with those from a newer release. See Zetetic's [Community Edition licensing guidance](https://www.zetetic.net/sqlcipher/community/).

## PDF.js and bundled resources

The app embeds PDF.js **6.3.289** and its resource files. Local PDF.js changes are described in [VENDOR_PATCHES.md](docs/VENDOR_PATCHES.md).

| Component | License / notice | Included text |
| --- | --- | --- |
| PDF.js | Apache-2.0 | [LICENSE](static/vendor/pdfjs/LICENSE) |
| Adobe CMaps | Adobe redistribution terms | [CMap LICENSE](static/vendor/pdfjs/cmaps/LICENSE) |
| Foxit/PDFium standard fonts | BSD-style redistribution terms | [LICENSE_FOXIT](static/vendor/pdfjs/standard_fonts/LICENSE_FOXIT) |
| Liberation fonts | GPL v2 with the exceptions stated in the bundled license | [LICENSE_LIBERATION](static/vendor/pdfjs/standard_fonts/LICENSE_LIBERATION) |
| ICC profiles | CC0 1.0 | [ICC LICENSE](static/vendor/pdfjs/iccs/LICENSE) |
| OpenJPEG | BSD-style terms | [LICENSE_OPENJPEG](static/vendor/pdfjs/wasm/LICENSE_OPENJPEG) |
| PDF.js OpenJPEG integration | BSD-style terms | [LICENSE_PDFJS_OPENJPEG](static/vendor/pdfjs/wasm/LICENSE_PDFJS_OPENJPEG) |
| JBIG2 / PDFium | BSD-style terms | [LICENSE_JBIG2](static/vendor/pdfjs/wasm/LICENSE_JBIG2) |
| PDF.js JBIG2 integration | BSD-style terms | [LICENSE_PDFJS_JBIG2](static/vendor/pdfjs/wasm/LICENSE_PDFJS_JBIG2) |
| qcms | MIT-style terms | [LICENSE_QCMS](static/vendor/pdfjs/wasm/LICENSE_QCMS) |
| PDF.js qcms integration | BSD-style terms | [LICENSE_PDFJS_QCMS](static/vendor/pdfjs/wasm/LICENSE_PDFJS_QCMS) |

The Liberation font license, including its exceptions and copyright notices, is preserved in full. Do not relabel these fonts as MIT-licensed. Likewise, retain the individual notices when distributing embedded fonts, CMaps, ICC profiles, or WASM components.

The complete corresponding Liberation Fonts 1.07.4 source archive (including SFD files and build scripts) is included under [licenses/sources](licenses/sources/README.md). The [manifest](licenses/sources/liberation-fonts-1.07.4.json) records official release URLs and hashes. Preserve this directory with binary distributions; `scripts/package.ps1` includes it automatically.

Upstream PDF.js source and resources: [mozilla/pdf.js](https://github.com/mozilla/pdf.js). Resource files remain under `static/vendor/pdfjs/`. The four Liberation Sans fonts have been replaced with unmodified official 1.07.4 release binaries; the JavaScript patch and font replacement are documented separately.

## External runtime

Microsoft Edge WebView2 Runtime is installed separately and is not included in this source repository. It remains subject to Microsoft's own terms; the Rust WebView2 bindings are listed separately in the Rust inventory.
