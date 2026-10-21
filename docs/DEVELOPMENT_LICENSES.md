# Development and optional-tool licenses

This source-repository document records tools used during development that are not included in the Windows app or Rust recovery executables. They are not shown in the app's **About & licenses** catalog or included in the binary distribution's notice inventory. The project's original code remains MIT-licensed.

## Rust inventory

The [complete Rust inventory](../licenses/rust/README.md) and [manifest](../licenses/rust/manifest.json) cover the app and both benchmark Cargo lockfiles, including runtime, build, and test dependencies. They retain original license expressions, upstream repositories, source URLs, notice files, and SHA-256 hashes.

Build/test-only helpers (such as `cc`, `embed-resource`, `syn`, and `quote`) and benchmark-only crates remain here for source auditing. The benchmark-only `io_tee 0.1.1` declares its dual license in its README; that declaration and the standard Apache-2.0 text are retained. Version-specific fallback texts are under [upstream](../licenses/upstream/), with provenance in [upstream-sources.json](../licenses/upstream-sources.json).

The app catalog follows normal dependency edges for Windows x64. Notices for directly used procedural macros are retained conservatively because they emit code into the app; their host-only dependencies are excluded. OpenSSL is a separate native component in the distribution inventory, despite being supplied through a build dependency. Do not remove native or generated-code notices solely because Cargo labels their provider as a build tool.

```powershell
node tools/licenses/generate.cjs
node tools/licenses/bundle.cjs
```

`generate.cjs` refreshes the full inventory and then calls `bundle.cjs`. Both use the local Cargo cache with `--locked --offline`. `bundle.cjs` only refreshes the distributed catalog and packaging manifest. Keep notice originals in the source repository even when excluded from the binary package.

## Optional Python tools

These packages are installed separately for the Python restorer or benchmark/image fixture generation. They are not bundled into the Windows executables. Direct restorer dependencies are pinned in [restore-requirements.txt](../tools/restore-requirements.txt).

| Package | Version | Declared license | Original notices |
| --- | --- | --- | --- |
| Tink | 1.16.1 | Apache-2.0 | [LICENSE](../licenses/python/tink-1.16.1/LICENSE) |
| cryptography | 50.0.1 | Apache-2.0 OR BSD-3-Clause | [Declaration](../licenses/python/cryptography-50.0.1/LICENSE), [Apache-2.0](../licenses/python/cryptography-50.0.1/LICENSE.APACHE), [BSD](../licenses/python/cryptography-50.0.1/LICENSE.BSD) |
| argon2-cffi | 25.1.0 | MIT | [LICENSE](../licenses/python/argon2-cffi-25.1.0/LICENSE) |
| Pillow | Unpinned; optional image generation | See upstream terms and nested notices | [Upstream snapshot](../licenses/python/Pillow-unpinned/LICENSE) |

Source-distribution URLs and hashes are recorded in [upstream-sources.json](../licenses/upstream-sources.json). This is a direct-dependency inventory, not a Python environment lockfile. If distributing an installed Python environment, retain the notices for its actual transitive dependencies and bundled native libraries too.

## Binary distribution

[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) and [DISTRIBUTION.md](../licenses/DISTRIBUTION.md) describe the Windows distribution. The app, Rust recovery GUI, and Rust recovery CLI share that catalog. `scripts/package.ps1` takes notice paths from `licenses/distribution.json`, including the complete corresponding Liberation font source. Development-only notices remain in this source repository.
