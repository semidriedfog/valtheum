# 개발 및 빌드

현재 빌드 대상은 Windows x64입니다. 실행 파일은 소스 저장소에 넣지 않고 별도 배포합니다.

## 준비

- Rust MSVC 도구 체인과 Cargo
- Visual Studio C++ Build Tools의 C++ 데스크톱 개발 도구와 Windows SDK
- 앱·GUI 복원 도구 실행용 Microsoft Edge WebView2 Runtime
- SQLCipher의 vendored OpenSSL을 빌드할 Perl
- 프런트엔드 테스트와 번역 생성용 Node.js

확인한 환경은 Rust/Cargo 1.93.0, Node.js 24.13.0, Windows x64입니다. 이는 확인한 조합이며 최소 지원 버전을 뜻하지 않습니다. Python은 독립 복원 도구·테스트 자료·아이콘 생성에만 필요하며 앱 빌드에는 필요하지 않습니다.

## 처음 빌드하기

Visual Studio 개발자 PowerShell 또는 MSVC 도구를 사용할 수 있는 PowerShell에서 프로젝트 루트로 이동합니다.

```powershell
rustc -Vv
cargo --version
perl --version
cargo fetch --locked
.\scripts\build.ps1
```

빌드 스크립트는 오프라인 모드이므로 최초에는 `cargo fetch --locked`로 의존성을 받아야 합니다. 다운로드 후에는 캐시를 재사용합니다. `Cargo.lock`은 저장소에 포함합니다.

SQLCipher는 `bundled-sqlcipher-vendored-openssl`로 빌드합니다. 로컬 OpenSSL 경로를 지정하지 않으면 의존성의 OpenSSL 소스를 빌드하므로 첫 빌드는 오래 걸릴 수 있습니다. Perl을 PATH에서 찾을 수 없다면 실행 파일의 절대 경로를 지정합니다.

```powershell
$env:OPENSSL_SRC_PERL = 'C:\tools\perl\bin\perl.exe'
.\scripts\build.ps1
```

MSVC용 vendored OpenSSL은 PATH의 NASM을 감지해 어셈블리 최적화를 사용합니다. NASM은 선택 사항입니다. 다른 PC에서 복사한 `target/`이나 개인 설정은 준비물에 포함하지 않습니다.

### 기존 정적 OpenSSL 사용

빌드 대상과 아키텍처가 같은 OpenSSL 설치를 사용합니다. 루트 아래에 `include/`와 `lib/libcrypto.lib` 등이 있어야 합니다.

```powershell
$env:OPENSSL_DIR = 'C:\tools\openssl'
$env:OPENSSL_STATIC = '1'
$env:OPENSSL_NO_VENDOR = '1'
.\scripts\build.ps1
```

위 경로는 예시이며 실제 설치 위치로 바꿉니다. 스크립트는 별도 환경 변수가 없으면 `target/build-tools/openssl`과 `target/build-tools/perl/perl/bin/perl.exe`를 재사용합니다. 이 디렉터리들은 로컬 캐시로 Git에 포함하지 않습니다.

### 산출물

| 파일 | 용도 |
| --- | --- |
| `target/release/valtheum.exe` | 배포용 앱 |
| `target/release/valtheum-restore.exe` | 독립 복원 GUI |
| `target/release/valtheum-restore-cli.exe` | 독립 복원 CLI |

Cargo 패키지·주 실행 대상 이름은 `vault`입니다. 빌드 스크립트가 `vault.exe`를 배포 이름인 `valtheum.exe`로 복사합니다. 두 파일을 모두 배포할 필요는 없습니다. 앱과 GUI의 release 실행은 콘솔 창을 표시하지 않으며 CLI는 콘솔을 사용합니다.

### 배포 ZIP 만들기

```powershell
.\scripts\package.ps1
```

글꼴·소스 해시를 검증하고 라이선스 카탈로그를 갱신한 뒤 release 빌드로 `dist/valtheum-windows-x64.zip`을 만듭니다. 실행 파일 세 개, 문서, 타사 고지, Liberation 1.07.4 전체 소스를 허용 목록으로 수집하고 ZIP 안의 각 파일을 원본 SHA-256과 대조합니다.

공개 배포에는 이 ZIP을 사용합니다. DB·키·설정·WebView 프로필은 포함하지 않습니다. `target/release/` 전체를 압축하지 마세요. README나 배포 문서를 수정한 뒤에도 스크립트를 다시 실행해 ZIP을 갱신합니다. 글꼴 변경 시 [대응 소스 관리 절차](../licenses/sources/README.md)도 확인합니다.

## 테스트

전체 검사 후 빌드:

```powershell
.\scripts\build.ps1 -Test
```

개별 실행은 OpenSSL 환경을 위와 같이 준비한 뒤 수행합니다.

```powershell
cargo test --locked --offline --release -- --test-threads=4
$testFiles = @(Get-ChildItem -LiteralPath tests -Filter '*.test.cjs' -File |
    Sort-Object Name | ForEach-Object { $_.FullName })
node --test @testFiles
cargo fmt --all -- --check
```

기본 검사에서 제외된 테스트에는 합성 호환성 자료 생성, 수동 브라우저 검사 서버, 하위 프로세스 보조 테스트가 있습니다. 전체 `--ignored` 실행 대신 [포맷 명세](VTA_FORMAT.md)의 이름별 명령을 사용합니다. 실제 라이브러리 대신 공개 합성 자료로 검증합니다.

개발용 실행:

```powershell
cargo run --locked --offline --bin vault
```

앱은 `127.0.0.1:8080`을 우선 사용하고 사용할 수 없으면 다른 로컬 포트를 할당합니다. `static/`과 `restore-ui/`는 실행 파일에 포함되므로 수정 후 재컴파일합니다.

## 번역과 PDF.js

기존 번역은 `tools/i18n/source-strings.json`과 `translations.tsv`, 추가 문구는 `tools/i18n/extra.json`에서 관리합니다.

```powershell
node tools/i18n/build.cjs
node --test tests/i18n.test.cjs
```

생성된 `static/i18n-catalog.js`와 `static/i18n-catalog.json`도 함께 반영합니다. 생성 파일만 직접 수정하지 않습니다.

PDF.js를 교체할 때는 [번들 패치 기록](VENDOR_PATCHES.md)을 확인하고 `tests/pdf-engine.test.cjs` 및 프런트엔드 테스트를 실행합니다. 글꼴·CMap·ICC·WASM 자원과 동봉 라이선스를 함께 유지합니다.

## GitHub에 올릴 파일

소스·문서·아이콘·Cargo.lock·공개 테스트 자료를 커밋합니다. 실행 파일은 별도 Releases 첨부물로 배포할 수 있습니다.

`.gitignore`는 DB·키·일반 미디어 확장자·개인 설정·WebView 프로필·빌드 캐시·로컬 도구 설정·`dist/`를 제외합니다. 앱 리소스, 공개 테스트 자료와 라이선스 소스 묶음은 예외로 유지합니다. `tests/fixtures/vta1/vault.key`와 `sample.vta`는 공개 합성 자료이며 실제 키가 아닙니다.

`.gitattributes`는 프로젝트 텍스트의 줄바꿈을 통일하되, `static/vendor/`, `licenses/`, `tests/fixtures/`는 해시 검증을 위해 바이트 그대로 보관합니다. `.editorconfig`도 같은 예외를 사용합니다. 기여 절차는 [CONTRIBUTING.md](../CONTRIBUTING.md)에 있습니다.

아직 Git 저장소가 아니라면 프로젝트 루트에서 다음과 같이 시작할 수 있습니다. 아래 명령은 소스 경로를 명시해 스테이징하며, 커밋·원격 등록·업로드는 별도로 수행합니다.

```powershell
git init -b main
git add -- .gitignore .gitattributes .editorconfig .github Cargo.toml Cargo.lock build.rs LICENSE CONTRIBUTING.md README.md README.en.md README.ja.md THIRD_PARTY_NOTICES.md valtheum.png src static restore-ui tests tools scripts assets docs licenses
```

이미 추적 중인 파일에는 `.gitignore`가 소급 적용되지 않습니다. 공개 전에 스테이징 목록과 제외 규칙을 확인하세요. 제외된 개인 자료를 올리기 위해 `git add -f`를 사용하지 않습니다.

```powershell
git status --short
git diff --cached --stat
git diff --cached --check
git ls-files
git check-ignore -v .media_index.db vault_config.json .valtheum/vault.key dist/valtheum-windows-x64.zip
```

프로젝트 자체 코드는 [MIT 라이선스](../LICENSE)입니다. 배포 구성 요소의 고지는 [타사 고지](../THIRD_PARTY_NOTICES.md)와 [배포 목록](../licenses/DISTRIBUTION.md)에서 관리합니다. 개발·벤치마크·선택 Python 도구의 전체 고지와 출처는 소스 저장소의 `docs/DEVELOPMENT_LICENSES.md`, `licenses/rust/manifest.json`, `licenses/python/`에 보존하며 앱 내 목록과 배포 ZIP에서는 제외합니다.

의존성을 바꾼 뒤 `node tools/licenses/generate.cjs`로 전체 Rust 목록과 앱 카탈로그를 갱신합니다. 고지 원문만 바꿨다면 `node tools/licenses/bundle.cjs`를 실행합니다. Windows 런타임 의존성과 직접 생성되는 코드의 관련 고지는 유지하고, OpenSSL 네이티브 고지는 별도로 포함합니다. 앱과 복원 GUI의 **정보·라이선스**, 복원 CLI의 `--license`·`--licenses`에서 배포 고지를 오프라인으로 확인할 수 있습니다. `licenses/distribution.json`이 실제 패키징할 고지·글꼴 소스 파일을 지정합니다. 생성 파일만 직접 수정하지 마세요.

## 정리할 수 있는 자료

| 위치 | 관리 기준 |
| --- | --- |
| `target/verification/` | 최신 테스트·빌드 로그만 남기고, 이전 로그·화면 캡처·일회성 재현 스크립트는 정리 가능 |
| `dist/` | 최신 배포 ZIP 보관. 이전 소스·배포 ZIP은 현재 소스와 일치하는지 확인 후 정리 |
| `target/build-tools/` | OpenSSL·Perl 등 재빌드에 사용하는 로컬 도구 보관 |
| `target/debug/`, `target/release/` | 빌드 캐시와 실행 파일. 앱 실행으로 생긴 설정·WebView 프로필이 섞일 수 있으므로 통째로 삭제하지 않음 |
| `tests/fixtures/`, `tools/*-crypto-bench/` | 공개 테스트 자료·재현 코드·기록된 측정 결과를 소스와 함께 보관 |
| `licenses/`, `docs/DEVELOPMENT_LICENSES.md` | 제품·개발 도구의 고지 원문과 출처를 보관. 개발 전용이라는 이유로 소스 저장소에서 삭제하지 않음 |

DB·미디어·`vault.key`·`.valtheum-password-change`는 캐시가 아닙니다. 실제 데이터 정리는 앱의 기능과 [백업 안내](SECURITY.md)를 따릅니다.
