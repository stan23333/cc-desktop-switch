param(
    [string]$Version = "1.1.0",
    [string]$OutputDir = "release",
    [switch]$Build,
    [switch]$TryInstaller,
    [switch]$CodeSign,
    [string]$CodeSigningCertificatePath,
    [string]$CodeSigningCertificatePassword,
    [string]$CodeSigningCertificateBase64,
    [string]$TimestampServer = "http://timestamp.digicert.com",
    [string]$Repository = $env:GITHUB_REPOSITORY
)

$ErrorActionPreference = "Stop"

function Get-ProjectRoot {
    return (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
}

function Convert-BytesToKeyFile {
    param(
        [byte[]]$Bytes,
        [string]$Label
    )
    $base64 = [Convert]::ToBase64String($Bytes)
    $lines = for ($i = 0; $i -lt $base64.Length; $i += 64) {
        $base64.Substring($i, [Math]::Min(64, $base64.Length - $i))
    }
    return "-----BEGIN $Label-----`n$($lines -join "`n")`n-----END $Label-----`n"
}

function Convert-KeyFileToBytes {
    param([string]$Text)
    $body = ($Text -split "`n" | Where-Object { $_ -notmatch "^-----" -and $_.Trim() }) -join ""
    return [Convert]::FromBase64String($body)
}

function Get-OrCreateSigningKey {
    param(
        [string]$KeyDir,
        [string]$ReleaseDir
    )

    New-Item -ItemType Directory -Force -Path $KeyDir | Out-Null
    $privatePath = Join-Path $KeyDir "release-private-key.pem"
    $publicPath = Join-Path $KeyDir "release-public-key.pem"

    if (-not (Test-Path -LiteralPath $privatePath)) {
        $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider 3072
        $rsa.PersistKeyInCsp = $false
        $privateText = Convert-BytesToKeyFile -Bytes $rsa.ExportCspBlob($true) -Label "RSA PRIVATE KEY BLOB"
        $publicText = Convert-BytesToKeyFile -Bytes $rsa.ExportCspBlob($false) -Label "RSA PUBLIC KEY BLOB"
        Set-Content -LiteralPath $privatePath -Value $privateText -Encoding ascii -NoNewline
        Set-Content -LiteralPath $publicPath -Value $publicText -Encoding ascii -NoNewline
        Write-Host "Created local release signing key: $privatePath"
    }

    Copy-Item -LiteralPath $publicPath -Destination (Join-Path $ReleaseDir "CC-Desktop-Switch-release-public.pem") -Force
    return $privatePath
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Sign-File {
    param(
        [string]$Path,
        [string]$PrivateKeyPath
    )

    $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider
    $rsa.PersistKeyInCsp = $false
    $pem = Get-Content -LiteralPath $PrivateKeyPath -Raw -Encoding ascii
    $rsa.ImportCspBlob((Convert-KeyFileToBytes -Text $pem))
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $signature = $rsa.SignData($bytes, [System.Security.Cryptography.CryptoConfig]::MapNameToOID("SHA256"))
    $sigPath = "$Path.sig"
    Set-Content -LiteralPath $sigPath -Value ([Convert]::ToBase64String($signature)) -Encoding ascii -NoNewline
    return $sigPath
}

function Add-Asset {
    param(
        [System.Collections.Generic.List[object]]$Assets,
        [string]$Path,
        [string]$PrivateKeyPath
    )

    $sha = Get-Sha256 -Path $Path
    $shaPath = "$Path.sha256"
    Set-Content -LiteralPath $shaPath -Value "$sha  $(Split-Path -Leaf $Path)" -Encoding ascii
    $sigPath = Sign-File -Path $Path -PrivateKeyPath $PrivateKeyPath

    $Assets.Add([ordered]@{
        name = Split-Path -Leaf $Path
        url = Get-AssetUrl -FileName (Split-Path -Leaf $Path)
        signature = Split-Path -Leaf $sigPath
        sha256 = $sha
        size = (Get-Item -LiteralPath $Path).Length
    }) | Out-Null
}

function Invoke-TauriBuild {
    param([string[]]$Arguments)

    $pnpm = Get-Command pnpm -ErrorAction SilentlyContinue
    if (-not $pnpm) {
        throw "pnpm not found. Install Node.js with corepack enabled before building Tauri release artifacts."
    }

    $tauriArgs = @("tauri", "build") + $Arguments
    & $pnpm.Source @tauriArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri build failed with exit code $LASTEXITCODE"
    }
}

function Get-TauriWindowsExecutable {
    param([string]$TauriReleaseDir)

    $candidate = Join-Path $TauriReleaseDir "cc-desktop-switch.exe"
    if (Test-Path -LiteralPath $candidate) {
        return $candidate
    }

    $match = Get-ChildItem -LiteralPath $TauriReleaseDir -File -Filter "*.exe" -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notmatch "setup|installer" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($match) {
        return $match.FullName
    }

    throw "Tauri Windows executable not found under $TauriReleaseDir"
}

function Get-TauriNsisInstaller {
    param([string]$TauriReleaseDir)

    $nsisDir = Join-Path $TauriReleaseDir "bundle\nsis"
    if (-not (Test-Path -LiteralPath $nsisDir)) {
        return $null
    }

    $match = Get-ChildItem -LiteralPath $nsisDir -File -Filter "*.exe" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($match) {
        return $match.FullName
    }
    return $null
}

function Add-MacPlatformAssets {
    param(
        [System.Collections.IDictionary]$Platforms,
        [string]$MacDir,
        [string]$ReleaseDir,
        [string]$Arch,
        [string]$Version,
        [string]$PrivateKeyPath
    )

    if (-not (Test-Path -LiteralPath $MacDir)) {
        return
    }

    $macAssets = [System.Collections.Generic.List[object]]::new()
    foreach ($extension in @("pkg", "dmg")) {
        $assetPath = Join-Path $MacDir "CC-Desktop-Switch-v$Version-macOS-$Arch.$extension"
        if (Test-Path -LiteralPath $assetPath) {
            $releaseAssetPath = Join-Path $ReleaseDir (Split-Path -Leaf $assetPath)
            Copy-Item -LiteralPath $assetPath -Destination $releaseAssetPath -Force
            Add-Asset -Assets $macAssets -Path $releaseAssetPath -PrivateKeyPath $PrivateKeyPath
        }
    }

    if ($macAssets.Count -gt 0) {
        $Platforms["macos-$Arch"] = [ordered]@{
            assets = $macAssets
        }
    }
}

function Get-AssetUrl {
    param([string]$FileName)
    if ($Repository) {
        return "https://github.com/$Repository/releases/download/v$Version/$FileName"
    }
    return $FileName
}

function Invoke-OptionalCodeSigning {
    param([string[]]$Files)

    if (-not $CodeSign) {
        return
    }

    $base64 = $CodeSigningCertificateBase64
    if (-not $base64 -and $env:WINDOWS_CODESIGN_PFX_BASE64) {
        $base64 = $env:WINDOWS_CODESIGN_PFX_BASE64
    }

    $password = $CodeSigningCertificatePassword
    if (-not $password -and $env:WINDOWS_CODESIGN_PFX_PASSWORD) {
        $password = $env:WINDOWS_CODESIGN_PFX_PASSWORD
    }

    $args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", (Join-Path $PSScriptRoot "Invoke-CodeSigning.ps1"),
        "-Files"
    ) + $Files + @("-TimestampServer", $TimestampServer)

    if ($CodeSigningCertificatePath) {
        $args += @("-CertificatePath", $CodeSigningCertificatePath)
    }
    if ($password) {
        $args += @("-CertificatePassword", $password)
    }
    if ($base64) {
        $args += @("-CertificateBase64", $base64)
    }

    & powershell @args
    if ($LASTEXITCODE -ne 0) {
        throw "Authenticode signing failed."
    }
}

$root = Get-ProjectRoot
$releaseDir = Join-Path $root $OutputDir
$distDir = Join-Path $root "dist"
$tauriReleaseDir = Join-Path $root "src-tauri\target\release"

Set-Location $root
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
foreach ($pattern in @(
    "CC-Desktop-Switch-v*-Windows-*",
    "CC-Desktop-Switch-v*-macOS-*",
    "CC-Desktop-Switch-release-public.pem",
    "latest.json",
    "latest.json.sha256",
    "latest.json.sig"
)) {
    Get-ChildItem -LiteralPath $releaseDir -File -Filter $pattern -ErrorAction SilentlyContinue |
        Remove-Item -Force
}

if ($Build) {
    Invoke-TauriBuild -Arguments @("--no-bundle", "--no-sign", "--ci")
    $builtExe = Get-TauriWindowsExecutable -TauriReleaseDir $tauriReleaseDir
    Invoke-OptionalCodeSigning -Files @($builtExe)
    Invoke-TauriBuild -Arguments @("--bundles", "nsis", "--no-sign", "--ci")
}

$tauriExe = Get-TauriWindowsExecutable -TauriReleaseDir $tauriReleaseDir
if ($CodeSign -and -not $Build) {
    Invoke-OptionalCodeSigning -Files @($tauriExe)
}

$licensePath = Join-Path $root "LICENSE.txt"
$tauriSetup = Get-TauriNsisInstaller -TauriReleaseDir $tauriReleaseDir
if ($TryInstaller -and -not $tauriSetup) {
    throw "Tauri NSIS installer not found under $(Join-Path $tauriReleaseDir "bundle\nsis")"
}

$keyDir = Join-Path $root ".release-signing"
$privateKey = Get-OrCreateSigningKey -KeyDir $keyDir -ReleaseDir $releaseDir
$assets = [System.Collections.Generic.List[object]]::new()

$portableZip = Join-Path $releaseDir "CC-Desktop-Switch-v$Version-Windows-Portable.zip"
if (Test-Path -LiteralPath $portableZip) { Remove-Item -LiteralPath $portableZip -Force }
$portableStage = Join-Path $releaseDir "portable-windows-x64"
if (Test-Path -LiteralPath $portableStage) {
    Remove-Item -LiteralPath $portableStage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $portableStage | Out-Null
Copy-Item -LiteralPath $tauriExe -Destination (Join-Path $portableStage "CC Desktop Switch.exe") -Force
if (Test-Path -LiteralPath $licensePath) {
    Copy-Item -LiteralPath $licensePath -Destination (Join-Path $portableStage "LICENSE.txt") -Force
}
Compress-Archive -Path (Join-Path $portableStage "*") -DestinationPath $portableZip -Force
Remove-Item -LiteralPath $portableStage -Recurse -Force
Add-Asset -Assets $assets -Path $portableZip -PrivateKeyPath $privateKey

$releaseExe = Join-Path $releaseDir "CC-Desktop-Switch-v$Version-Windows-x64.exe"
Copy-Item -LiteralPath $tauriExe -Destination $releaseExe -Force

$releaseSetup = $null
if ($tauriSetup -and (Test-Path -LiteralPath $tauriSetup)) {
    $releaseSetup = Join-Path $releaseDir "CC-Desktop-Switch-v$Version-Windows-Setup.exe"
    Copy-Item -LiteralPath $tauriSetup -Destination $releaseSetup -Force
}

if ($CodeSign -and $releaseSetup -and (Test-Path -LiteralPath $releaseSetup)) {
    Invoke-OptionalCodeSigning -Files @($releaseSetup)
}

Add-Asset -Assets $assets -Path $releaseExe -PrivateKeyPath $privateKey
if ($releaseSetup -and (Test-Path -LiteralPath $releaseSetup)) {
    Add-Asset -Assets $assets -Path $releaseSetup -PrivateKeyPath $privateKey
}

$platforms = [ordered]@{
    "windows-x64" = [ordered]@{
        assets = $assets
    }
}
$macDistDir = Join-Path $distDir "mac"
Add-MacPlatformAssets -Platforms $platforms -MacDir $macDistDir -ReleaseDir $releaseDir -Arch "arm64" -Version $Version -PrivateKeyPath $privateKey
Add-MacPlatformAssets -Platforms $platforms -MacDir $macDistDir -ReleaseDir $releaseDir -Arch "x64" -Version $Version -PrivateKeyPath $privateKey

$latest = [ordered]@{
    name = "CC Desktop Switch"
    version = $Version
    pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    notes = "Windows release for CC Desktop Switch v$Version."
    update_protocol = 1
    minimum_supported_version = "1.0.0"
    platforms = $platforms
    signature = [ordered]@{
        algorithm = "RSA-CSP-BLOB-SHA256"
        public_key = "CC-Desktop-Switch-release-public.pem"
        format = "base64 raw signature over file bytes"
    }
}

$latestPath = Join-Path $releaseDir "latest.json"
$latestJson = $latest | ConvertTo-Json -Depth 8
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($latestPath, $latestJson, $utf8NoBom)
Add-Asset -Assets ([System.Collections.Generic.List[object]]::new()) -Path $latestPath -PrivateKeyPath $privateKey | Out-Null

Get-ChildItem -LiteralPath $releaseDir -File | Sort-Object Name | Select-Object Name, Length
