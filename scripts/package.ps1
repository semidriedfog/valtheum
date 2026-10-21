# Build a public distribution from an explicit allowlist, never the whole release directory.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    node tools/licenses/verify-fonts.cjs
    if ($LASTEXITCODE -ne 0) { throw 'Font source verification failed' }
    node tools/licenses/bundle.cjs
    if ($LASTEXITCODE -ne 0) { throw 'License catalog generation failed' }
    & (Join-Path $PSScriptRoot 'build.ps1')

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [void][IO.Directory]::CreateDirectory((Join-Path $projectRoot 'dist'))
    $files = [ordered]@{}
    foreach ($name in @('valtheum.exe', 'valtheum-restore.exe', 'valtheum-restore-cli.exe')) {
        $files[$name] = Join-Path $projectRoot "target/release/$name"
    }
    foreach ($name in @('README.md', 'README.en.md', 'README.ja.md', 'CONTRIBUTING.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md')) {
        $files[$name] = Join-Path $projectRoot $name
    }
    foreach ($name in @('docs/BUILD.md', 'docs/SECURITY.md', 'docs/VTA_FORMAT.md', 'docs/VENDOR_PATCHES.md')) {
        $files[$name] = Join-Path $projectRoot $name
    }
    $noticeManifest = Get-Content -LiteralPath (Join-Path $projectRoot 'licenses/distribution.json') -Raw | ConvertFrom-Json
    foreach ($name in $noticeManifest.files) {
        $source = [IO.Path]::GetFullPath((Join-Path $projectRoot $name))
        if (-not $source.StartsWith($projectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Notice path escapes project: $name"
        }
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing distribution notice: $name" }
        $files[$name] = $source
    }
    $stage = Join-Path $projectRoot ('dist/valtheum-package-' + [guid]::NewGuid().ToString('N') + '.zip')
    $zip = [IO.Compression.ZipFile]::Open($stage, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entry in $files.GetEnumerator()) {
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $entry.Value, $entry.Key, [IO.Compression.CompressionLevel]::Optimal)
        }
    } finally { $zip.Dispose() }
    $zip = [IO.Compression.ZipFile]::OpenRead($stage)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        if ($zip.Entries.Count -ne $files.Count) { throw 'ZIP entry count mismatch' }
        foreach ($entry in $zip.Entries) {
            $stream = $entry.Open()
            try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') } finally { $stream.Dispose() }
            if ($actual -ne (Get-FileHash -LiteralPath $files[$entry.FullName] -Algorithm SHA256).Hash) {
                throw "ZIP verification failed: $($entry.FullName)"
            }
        }
    } finally { $sha.Dispose(); $zip.Dispose() }
    $destination = Join-Path $projectRoot 'dist/valtheum-windows-x64.zip'
    Move-Item -LiteralPath $stage -Destination $destination -Force
    Write-Output "Verified distribution: $destination ($($files.Count) files)"
} finally { Pop-Location }
