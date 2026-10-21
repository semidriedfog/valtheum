<p align="center"><img src="valtheum.png" width="96" alt="valtheum icon"></p>

# valtheum

[한국어](README.md) · **English** · [日本語](README.ja.md)

**A Windows app for managing videos, photos, image archives, PDFs, and encrypted notes in local folders.**

Built with Rust and WebView2. No account or external server is required to use a local library. The app supports English (default), Korean, and Japanese.

**Passwords and encryption are optional.** You can view videos, images, image archives, and PDFs and manage the catalog, tags, ratings, and series without encryption. Database encryption and a password are required only when using media encryption or encrypted notes.

[Getting started](#getting-started) · [Supported formats](#supported-formats) · [Encryption and backups](#encryption-and-backups) · [Build guide (Korean)](docs/BUILD.md) · [Security and recovery (Korean)](docs/SECURITY.md)

## Features

- **Media viewing** — Play videos, browse images inside archives in natural numeric order, view individual images, and read PDFs with two-page viewing and last-page resume.
- **Library organization** — A virtualized scrolling list, filename and series search, separate tag search, sorting by rating, score, or date, a trash bin, and deletion history.
- **Series and arena** — Sort multiple files as one series and share tags, ratings, and scores. The arena compares only the representative file of each series.
- **Optional encryption** — SQLCipher databases, separate encryption for image archives, videos, and images, and standalone encrypted notes. Enable a password only for library folders you want to protect.
- **Recovery tools** — Database and key backups, restoration into a new folder, standalone media recovery GUI and CLI, quick integrity checks, and NAS reconnection.
- **Photo gallery** — Choose a separate photo folder for the startup screen, or display its images in a full-screen picture-frame mode with an adjustable interval.

## Getting started

Windows and the **Microsoft Edge WebView2 Runtime** are required. If you downloaded the source, [build the executables](#building-from-source) first. Executables are not included in the source repository.

| Executable | Purpose |
| --- | --- |
| `valtheum.exe` | Media library app |
| `valtheum-restore.exe` | GUI for restoring encrypted media without the app database |
| `valtheum-restore-cli.exe` | Command-line recovery tool; does not require WebView2 |

1. Run `valtheum.exe`. The photo gallery appears first.
2. Click the **Photo library** label **five times within 2.5 seconds**.
3. Select the actual library folder. If its database is encrypted, enter that folder's password.
4. Synchronize the folder, then select media from the list.

The actual library folder list is kept only for the current session. Select the folder again after restarting the app. The public gallery folder, picture-frame interval, and language are saved in `vault_config.json`. No default photos are bundled.

## Supported formats

| Type | Support |
| --- | --- |
| Video | MP4, MKV, WebM, AVI, MOV, M4V. Playback depends on WebView2 codec support. |
| Individual images | JPG, JPEG, PNG, WebP, GIF, BMP, AVIF |
| Image archives | Images inside ZIP and TAR.ZST files |
| PDF | Fit page or width, zoom, two-page viewing, mouse-wheel navigation, page progress slider, last-page resume, password entry |
| Encrypted files | Image archives `.vta`, videos `.vtv`, images `.vti`, standalone notes `.vtn` |

RAR and 7z files can be listed but cannot be opened. Standalone `.zst` and `.cbz` files are not supported. TAR.ZST requires sequential decompression, so later pages can be slower to load. Archive limits are 32 MiB per decompressed image and 100,000 entries.

PDF.js and fonts are bundled for offline PDF viewing. PDF scripts, external links, form editing, text search, and encryption of the PDF file itself are not supported.

## Usage tips

- The current media item is highlighted in **orange**; series have a **gold border**.
- Use **V / B** to select an item in the arena. Only each series' representative participates.
- Scroll the mouse wheel over a video to seek. For image archives and PDFs, the wheel turns pages.
- Saved pages and playback positions appear over the media without shrinking the viewing area. If the bookmark list fails to load, use **Reload saved positions** to check again.
- Series changes continue saving after the window closes. Reopen it to check progress; if the outcome is uncertain, use **Check save result**. Further saves or series dissolution stay blocked until the result is confirmed.
- A view is counted after 10 seconds of actual video playback, or after viewing five distinct pages in an image archive or PDF.
- **＋ New note** creates a standalone note without a photo. Notes support title search, tags, ratings, and the trash bin, but do not participate in the arena or series.
- Encrypted image archives support display-name and restoration-filename changes, as well as deletion of multiple selected images. Back up before editing.

## Encryption and backups

| Usage | Password and encryption |
| --- | --- |
| Ordinary media viewing and management | No library password required. The database and media remain unencrypted. |
| Database encryption only | Optional. A folder password protects catalog data such as tags; existing media files stay unchanged. |
| Media encryption and encrypted notes | Unlock an encrypted database first. Notes are always stored encrypted. |

A password-protected PDF still requires its own password, independently of the library settings.

**Database encryption and media encryption are separate operations.** The database stores the catalog, tags, ratings, thumbnails, and other metadata; media contents remain in files. Encrypt and unlock the database before encrypting media or saving notes.

| Item to keep | Contents |
| --- | --- |
| An app-created `.vtbackup` | The current folder's database and, if created, `.valtheum/vault.key` |
| `.valtheum/archives`, `videos`, `images`, `notes` | Encrypted media contents that must be backed up separately |
| Plaintext media | Unencrypted originals; back these up separately as needed |

**A database backup alone cannot restore media or note contents.** Keep the database, key, and media together, and update both backups after editing media. Independent media recovery requires the encrypted file, its matching `vault.key`, and the key's password.

Encryption creates and verifies a new copy and **does not automatically delete the plaintext original**. Verify your backups and playback before using the original-file cleanup feature. Change passwords within the app so the change applies to both the database and key.

Quick integrity checks examine file existence and size, database/key consistency, encrypted metadata, and boundary blocks. **They do not check all content or repair damaged files.** The startup gallery does not hide files in Explorer or erase OS records.

See the [security, backup, and recovery guide (Korean)](docs/SECURITY.md) for procedures and protection limits, and the [encrypted format specification (English)](docs/VTA_FORMAT.md) for format details.

## Building from source

Prepare the Windows MSVC Rust toolchain, Visual Studio C++ Build Tools with the Windows SDK, and Perl for building OpenSSL. Node.js is required for frontend tests.

From PowerShell in the project root:

```powershell
cargo fetch --locked
.\scripts\build.ps1
```

Executables are generated in `target/release/`. The app UI is embedded in the executable, so rebuild after changing UI code. The [development and build guide (Korean)](docs/BUILD.md) covers initial setup, using an existing OpenSSL installation, tests, and distribution.

## Repository layout

| Path | Contents |
| --- | --- |
| `src/` | Rust app, encryption, recovery code, and tests |
| `static/`, `restore-ui/` | App and standalone recovery UI, plus bundled resources |
| `tests/` | JavaScript tests and public synthetic fixtures |
| `tools/` | Translation generation, Python recovery, and encryption benchmarks |
| `scripts/`, `assets/` | Build and icon-generation scripts, and Windows resources |
| `docs/` | Build instructions, security and recovery, file formats, and PDF.js patch notes |

Benchmark conditions and limitations (Korean): [image archives](tools/archive-crypto-bench/README.md) · [video](tools/video-crypto-bench/README.md). Bundled PDF.js changes are documented in the [patch notes (English)](docs/VENDOR_PATCHES.md).

## Public fixtures and licensing

For bug reports, proposed changes and validation, see [Contributing](CONTRIBUTING.md).

Personal databases, settings, keys, media, build output, and distribution packages are excluded by [.gitignore](.gitignore). The key and encrypted sample in `tests/fixtures/vta1/` are **public synthetic test fixtures** and must never be used for real data.

The project’s original code is licensed under the **[MIT License](LICENSE)**. Third-party code, PDF.js, fonts, and WASM components retain their own licenses. See **[Third-party licenses and notices](THIRD_PARTY_NOTICES.md)** for the versioned dependency inventory and original license texts.

Key third-party components include SQLCipher (BSD-3-Clause), PDF.js and OpenSSL (Apache-2.0), and Liberation fonts (GPL v2 with the bundled exceptions). Do not label the entire distribution as MIT-only. When redistributing, retain the MIT license and copyright notice, third-party notices, and required corresponding source.

Use **About & licenses** in the photo library, media library, or standalone recovery tool to search and read notices for distributed components offline. Development-only and benchmark-only tools are excluded from this catalog. The recovery CLI supports `--license` (MIT) and `--licenses` (distribution notices).

For redistribution, use `dist/valtheum-windows-x64.zip` generated by `scripts/package.ps1`. It includes the executables, third-party notices, and corresponding Liberation font source.
