param([switch]$Test)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (-not $env:OPENSSL_SRC_PERL) {
        $localPerl = Join-Path $projectRoot 'target/build-tools/perl/perl/bin/perl.exe'
        if (Test-Path -LiteralPath $localPerl) { $env:OPENSSL_SRC_PERL = $localPerl }
    }
    # Reuse a verified locally built static OpenSSL installation across profiles.
    if (-not $env:OPENSSL_DIR) {
        $localCrypto = Join-Path $projectRoot 'target/build-tools/openssl'
        if (Test-Path -LiteralPath (Join-Path $localCrypto 'lib/libcrypto.lib')) {
            $env:OPENSSL_DIR = $localCrypto
            $env:OPENSSL_STATIC = '1'
            $env:OPENSSL_NO_VENDOR = '1'
        }
    }
    if ($Test) {
        cargo test --locked --offline
        if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
        $testFiles = @(Get-ChildItem -LiteralPath (Join-Path $projectRoot 'tests') -Filter '*.test.cjs' -File | Sort-Object Name | ForEach-Object { $_.FullName })
        if ($testFiles.Count -eq 0) { throw 'No frontend tests found' }
        node --test @testFiles
        if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
        cargo fmt --all -- --check
        if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed' }
    }
    cargo build --locked --offline --release
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    Copy-Item -LiteralPath 'target/release/vault.exe' -Destination 'target/release/valtheum.exe' -Force
} finally { Pop-Location }
