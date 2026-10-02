#requires -Version 5.1
# Per-user Windows installation; no elevation, execution-policy changes or audio activation.
[CmdletBinding()]
param(
    [string]$From = '',
    [string]$Prefix = '',
    [string]$Version = '',
    [switch]$Build,
    [switch]$AllowUnsigned,
    [switch]$Yes,
    [switch]$DryRun,
    [switch]$Help
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($Help) {
    @'
Usage: .\install.ps1 [-Version vX.Y.Z] [-Prefix PATH] [-Yes] [-DryRun]
Default: download the latest approved CI-built native Windows release. No Rust/compiler required.
-From PATH and -Build are explicit offline/developer alternatives, not online fallbacks.
An offline source is a Maris directory containing bin\maris.exe and .maris-package.
Default destination: %LOCALAPPDATA%\Programs\Maris. No administrator account is required.
-From and -Build are mutually exclusive. -DryRun never builds or writes files.
Unsigned development payloads require explicit -AllowUnsigned; signed payloads use Authenticode.
An upgrade retains its previous payload. No PATH, drivers, services, login items or audio settings are changed.
Maris uses WASAPI process loopback for its Windows system-audio path; installation itself does not start or reroute audio.
'@
    return
}
if ($env:OS -ne 'Windows_NT') { throw 'This installer requires Windows; use install.sh on macOS or Linux.' }
if ($Build -and $PSBoundParameters.ContainsKey('From')) { throw '-From and -Build are mutually exclusive.' }
$online = -not $Build -and -not $PSBoundParameters.ContainsKey('From')
if ($Version) {
    if (-not $online) { throw '-Version cannot be combined with -From or -Build.' }
    $Version = $Version -creplace '^v', ''
    if ($Version -cnotmatch '^\d+\.\d+\.\d+$') { throw '-Version must be a stable X.Y.Z or vX.Y.Z.' }
}
if ($online -and $AllowUnsigned) { throw 'Online installation cannot bypass release verification. Use -From for explicit development/offline packages.' }
if ([string]::IsNullOrWhiteSpace($env:USERPROFILE)) { throw 'USERPROFILE is required.' }
if ([string]::IsNullOrWhiteSpace($Prefix)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw 'LOCALAPPDATA is required.' }
    $Prefix = Join-Path $env:LOCALAPPDATA 'Programs'
}
function Safe-Path([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value -match '[\x00-\x1f*?]') {
        # Standard extended local paths from native tests are normalized below.
        if (-not ($Value.StartsWith('\\?\') -and $Value.Substring(4) -notmatch '[\x00-\x1f*?]')) {
            throw 'Empty, wildcard or control-character path rejected.'
        }
    }
    if ($Value.StartsWith('\\?\')) { $Value = $Value.Substring(4) }
    if ($Value.StartsWith('\\') -or $Value -match '(^|[\\/])\.\.?([\\/]|$)') { throw 'UNC and traversal paths are not accepted.' }
    $full = [IO.Path]::GetFullPath($Value).TrimEnd([char[]]'\/')
    if ($full -notmatch '^[A-Za-z]:\\' -or $full.Substring(2).Contains(':')) { throw 'A local drive path is required; alternate streams are rejected.' }
    $current = $full
    while ($current.Length -gt 3) {
        $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Linked path component rejected: $current"
        }
        $current = [IO.Path]::GetDirectoryName($current)
        if ([string]::IsNullOrEmpty($current)) { break }
    }
    return $full
}
function Payload-Files([string]$Path) {
    if (-not [IO.Directory]::Exists($Path)) { throw "Missing local payload: $Path" }
    $stack = New-Object 'System.Collections.Generic.Stack[System.IO.DirectoryInfo]'
    $stack.Push([IO.DirectoryInfo]::new($Path))
    $files = New-Object 'System.Collections.Generic.List[System.IO.FileInfo]'
    [long]$total = 0; $count = 0
    while ($stack.Count -gt 0) {
        foreach ($entry in $stack.Pop().EnumerateFileSystemInfos()) {
            $count++
            if ($count -gt 10000) { throw 'Payload file-count limit exceeded.' }
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Linked or junction payload rejected.' }
            if ($entry -is [IO.DirectoryInfo]) { $stack.Push($entry) }
            elseif ($entry -is [IO.FileInfo]) {
                $total += $entry.Length
                if ($total -gt 536870912) { throw 'Payload size limit exceeded.' }
                $files.Add($entry)
            } else { throw 'Special payload entry rejected.' }
        }
    }
    if ($files.Count -eq 0) { throw 'Empty payload rejected.' }
    $files | Sort-Object FullName
}
$native = $env:PROCESSOR_ARCHITEW6432
if ([string]::IsNullOrEmpty($native)) { $native = $env:PROCESSOR_ARCHITECTURE }
switch ($native.ToUpperInvariant()) {
    'AMD64' { $arch = 'x86_64'; $machine = 0x8664 }
    'ARM64' { $arch = 'arm64'; $machine = 0xaa64 }
    default { throw 'Only native x86_64 and arm64 Windows packages are supported.' }
}
function Check-Marker([string]$Path) {
    $marker = Join-Path $Path '.maris-package'
    if (-not [IO.File]::Exists($marker) -or (Get-Item -LiteralPath $marker).Length -gt 128) { throw 'Missing or oversized Maris package identity.' }
    $lines = [IO.File]::ReadAllLines($marker)
    if ($lines.Length -ne 4 -or $lines[0] -cne 'maris-package-v1' -or $lines[1] -cne 'windows' -or
        $lines[2] -cne $arch -or $lines[3] -notmatch '^\d+\.\d+\.\d+$') { throw 'Incorrect Maris package platform, architecture or version.' }
}
function Validate-Payload([string]$Path) {
    $null = Payload-Files $Path
    Check-Marker $Path
    $binary = Join-Path $Path 'bin\maris.exe'
    $stream = [IO.File]::OpenRead($binary)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 128 -or $reader.ReadUInt16() -ne 0x5a4d) { throw 'Not a PE executable.' }
        $stream.Position = 0x3c
        [long]$offset = $reader.ReadUInt32()
        if ($offset -lt 64 -or $offset + 26 -gt $stream.Length) { throw 'Invalid PE header offset.' }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne $machine) { throw 'PE architecture does not match this Windows host.' }
        $stream.Position = $offset + 22
        $characteristics = $reader.ReadUInt16()
        if (($characteristics -band 2) -eq 0 -or ($characteristics -band 0x2000) -ne 0 -or $reader.ReadUInt16() -ne 0x20b) { throw 'Expected a 64-bit executable image, not a DLL.' }
    } finally { $reader.Dispose(); $stream.Dispose() }
    if (-not $AllowUnsigned) {
        $signature = Get-AuthenticodeSignature -LiteralPath $binary
        if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) { throw 'Authenticode validation failed. Use -AllowUnsigned only for trusted local development.' }
    }
}
function Payload-Digest([string]$Path) {
    $lines = foreach ($file in @(Payload-Files $Path)) {
        $relative = $file.FullName.Substring($Path.Length).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        "$relative`0$hash"
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}
# Online acquisition is separate from installation. Only owned temporary files are touched here.
function Read-MarisManifest([string]$Text, [string]$Architecture, [string]$RequestedVersion) {
    if ($Text.Length -gt 32768 -or $Text.Contains("`r")) { throw 'Invalid manifest size or line endings.' }
    $rows = $Text.TrimEnd([char]10).Split([char]10)
    if ($rows.Count -ne 10 -or $rows[0] -cne 'maris-release-v1') { throw 'Incomplete release manifest.' }
    $versionRow = $rows[1].Split([char]9); $sourceRow = $rows[2].Split([char]9); $channelRow = $rows[3].Split([char]9)
    if ($versionRow.Count -ne 2 -or $versionRow[0] -cne 'version' -or $versionRow[1] -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version.' }
    if ($sourceRow.Count -ne 2 -or $sourceRow[0] -cne 'source_sha256' -or $sourceRow[1] -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid release source digest.' }
    if ($channelRow.Count -ne 2 -or $channelRow[0] -cne 'channel' -or $channelRow[1] -cne 'stable') { throw 'Unapproved release channel rejected.' }
    $resolvedVersion = $versionRow[1]
    if ($RequestedVersion -and $RequestedVersion -cne $resolvedVersion) { throw 'Requested version and release manifest differ.' }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $selected = $null
    foreach ($row in $rows[4..9]) {
        $parts = $row.Split([char]9)
        if ($parts.Count -ne 7 -or $parts[0] -cne 'asset' -or $parts[1] -cnotmatch '^(macos|linux|windows)$' -or $parts[2] -cnotmatch '^(x86_64|arm64)$') { throw 'Invalid asset target.' }
        $extension = if ($parts[1] -ceq 'windows') { '.zip' } else { '.tar.gz' }
        $name = "Maris-$resolvedVersion-$($parts[1])-$($parts[2])$extension"
        if ($parts[3] -cne $name -or $parts[4] -cnotmatch '^[a-f0-9]{64}$' -or $parts[6] -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid asset name or checksum.' }
        if ($parts[5] -cnotmatch '^[1-9][0-9]{0,8}$' -or [long]$parts[5] -gt 134217728) { throw 'Invalid asset size.' }
        if (-not $seen.Add($parts[1] + '/' + $parts[2])) { throw 'Duplicate release target.' }
        if ($parts[1] -ceq 'windows' -and $parts[2] -ceq $Architecture) {
            $selected = [pscustomobject]@{ Version=$resolvedVersion; Source=$sourceRow[1]; Name=$name; Sha256=$parts[4]; Bytes=[long]$parts[5]; BinarySha256=$parts[6] }
        }
    }
    if ($null -eq $selected) { throw 'No approved release for this Windows architecture.' }
    return $selected
}
function Receive-ReleaseFile([string]$Url, [string]$Output, [long]$Limit) {
    Add-Type -AssemblyName System.Net.Http
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    for ($attempt = 0; $attempt -lt 3; $attempt++) {
        $handler = [Net.Http.HttpClientHandler]::new(); $handler.AllowAutoRedirect = $false
        $client = [Net.Http.HttpClient]::new($handler)
        $client.DefaultRequestHeaders.UserAgent.ParseAdd('Maris-Installer/1')
        $cancel = [Threading.CancellationTokenSource]::new(); $cancel.CancelAfter(180000)
        $response = $null; $inputStream = $null; $outputStream = $null; $request = $null; $retryable = $true
        try {
            $uri = [Uri]::new($Url)
            for ($redirect = 0; $redirect -le 5; $redirect++) {
                if ($uri.Scheme -cne 'https' -or $uri.UserInfo) { $retryable=$false; throw 'HTTPS-only release URLs are required.' }
                $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $uri)
                $response = $client.SendAsync($request, [Net.Http.HttpCompletionOption]::ResponseHeadersRead, $cancel.Token).GetAwaiter().GetResult()
                $status = [int]$response.StatusCode
                if ($status -in @(301,302,303,307,308)) {
                    if ($redirect -eq 5 -or $null -eq $response.Headers.Location) { $retryable=$false; throw 'Invalid or excessive release redirects.' }
                    $uri = [Uri]::new($uri, $response.Headers.Location)
                    $response.Dispose(); $response=$null; $request.Dispose(); $request=$null
                    continue
                }
                if ($status -ne 200) {
                    $retryable = $status -in @(408,429,500,502,503,504)
                    throw "Release download failed (HTTP $status). No approved asset is available; no compilation fallback."
                }
                break
            }
            $length = $response.Content.Headers.ContentLength
            if ($null -ne $length -and $length -gt $Limit) { $retryable=$false; throw 'Download exceeds its size limit.' }
            $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $outputStream = [IO.File]::Open($Output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
            $buffer = New-Object byte[] 65536
            [long]$total = 0
            while (($count = $inputStream.ReadAsync($buffer, 0, $buffer.Length, $cancel.Token).GetAwaiter().GetResult()) -gt 0) {
                $total += $count
                if ($total -gt $Limit) { $retryable=$false; throw 'Streamed download exceeds its size limit.' }
                $outputStream.Write($buffer, 0, $count)
            }
            if ($null -ne $length -and $total -ne $length) { throw 'Incomplete release transfer.' }
            return
        } catch {
            if (-not $retryable -or $attempt -eq 2) { throw }
        } finally {
            foreach ($resource in @($outputStream,$inputStream,$response,$request,$cancel,$client,$handler)) { if ($null -ne $resource) { $resource.Dispose() } }
        }
        if ([IO.File]::Exists($Output)) { [IO.File]::Delete($Output) }
        Start-Sleep -Seconds ($attempt + 1)
    }
}
function Expand-MarisArchive([string]$Archive, [string]$Destination, [string]$RootName) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -gt 10000 -or $zip.Entries.Count -eq 0) { throw 'Archive file-count limit rejected.' }
        $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        [long]$total = 0
        foreach ($entry in $zip.Entries) {
            $path = $entry.FullName.TrimEnd([char]47)
            if ($path -cnotmatch '^[A-Za-z0-9_./-]+$' -or ($path -cne $RootName -and -not $path.StartsWith($RootName + '/', [StringComparison]::Ordinal))) { throw 'Archive path is outside the expected package.' }
            foreach ($component in $path.Split([char]47)) {
                if (-not $component -or $component -in @('.','..') -or $component.EndsWith('.') -or $component -match '^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$') { throw 'Ambiguous or reserved archive path rejected.' }
            }
            if (-not $seen.Add($path)) { throw 'Duplicate or case-colliding archive entry.' }
            $type = ($entry.ExternalAttributes -shr 16) -band 0xf000
            if ($type -notin @(0,0x8000,0x4000) -or ($entry.ExternalAttributes -band 0x400) -ne 0) { throw 'Linked or special archive entry rejected.' }
            $directory = $entry.FullName.EndsWith('/')
            if (($directory -and $type -eq 0x8000) -or (-not $directory -and $type -eq 0x4000) -or ($directory -and $entry.Length -ne 0)) { throw 'Archive entry type mismatch.' }
            $total += $entry.Length
            if ($total -gt 536870912) { throw 'Expanded archive size limit exceeded.' }
        }
        foreach ($entry in $zip.Entries) {
            $path = Join-Path $Destination $entry.FullName.Replace('/', '\')
            if ($entry.FullName.EndsWith('/')) { $null=[IO.Directory]::CreateDirectory($path); continue }
            $null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
            $inputStream = $entry.Open(); $outputStream = $null
            try {
                $outputStream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
                $buffer = New-Object byte[] 65536; [long]$written = 0
                while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $written += $count
                    if ($written -gt $entry.Length) { throw 'Archive expanded beyond its declared size.' }
                    $outputStream.Write($buffer, 0, $count)
                }
                if ($written -ne $entry.Length) { throw 'Truncated archive entry.' }
            } finally { $inputStream.Dispose(); if ($null -ne $outputStream) { $outputStream.Dispose() } }
        }
    } finally { $zip.Dispose() }
}
function Receive-MarisPackage([string]$RequestedVersion, [string]$Architecture) {
    $base = 'https://github.com/francis-du/maris/releases'
    $manifestUrl = if ($RequestedVersion) { "$base/download/v$RequestedVersion/maris-release.tsv" } else { "$base/latest/download/maris-release.tsv" }
    $work = Safe-Path (Join-Path ([IO.Path]::GetTempPath()) ('maris-download.' + [Guid]::NewGuid().ToString('N')))
    if (Test-Path -LiteralPath $work) { throw 'Temporary download destination already exists.' }
    $null = [IO.Directory]::CreateDirectory($work)
    try {
        $manifest = Join-Path $work 'manifest.tsv'
        Receive-ReleaseFile $manifestUrl $manifest 32768
        $asset = Read-MarisManifest ([IO.File]::ReadAllText($manifest)) $Architecture $RequestedVersion
        $archive = Join-Path $work 'package.zip'
        Write-Output "Downloading CI-built Maris $($asset.Version), Windows/$Architecture" | Out-Host
        Receive-ReleaseFile "$base/download/v$($asset.Version)/$($asset.Name)" $archive $asset.Bytes
        if ((Get-Item -LiteralPath $archive).Length -ne $asset.Bytes -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne $asset.Sha256) { throw 'Archive size or SHA-256 mismatch; extraction refused.' }
        $rootName = "Maris-$($asset.Version)-windows-$Architecture"
        Expand-MarisArchive $archive (Join-Path $work 'extracted') $rootName
        $kit = Join-Path (Join-Path $work 'extracted') $rootName
        $marker = Join-Path $kit '.maris-release'
        if (-not [IO.File]::Exists($marker) -or (Get-Item -LiteralPath $marker).Length -gt 512) { throw 'Missing or oversized release identity.' }
        $expected = @('maris-install-kit-v1',$asset.Version,'windows',$Architecture,$asset.Source,'stable',$asset.BinarySha256)
        $actual = [IO.File]::ReadAllLines($marker)
        if ($actual.Count -ne $expected.Count -or ($actual -join "`n") -cne ($expected -join "`n")) { throw 'Package identity/channel differs from its approved manifest.' }
        $payload = Join-Path $kit 'Maris'
        $packageMarker = Join-Path $payload '.maris-package'
        if (-not [IO.File]::Exists($packageMarker) -or (Get-Item -LiteralPath $packageMarker).Length -gt 128) { throw 'Missing or oversized native payload identity.' }
        $packageLines = [IO.File]::ReadAllLines($packageMarker)
        if ($packageLines.Count -ne 4 -or ($packageLines -join "`n") -cne "maris-package-v1`nwindows`n$Architecture`n$($asset.Version)") { throw 'Native payload version differs from its release manifest.' }
        if ((Get-FileHash -LiteralPath (Join-Path $payload 'bin\maris.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -cne $asset.BinarySha256) { throw 'Executable SHA-256 mismatch.' }
        return [pscustomobject]@{ Directory=$work; Payload=$payload }
    } catch {
        if ([IO.Directory]::Exists($work)) { Remove-Item -LiteralPath $work -Recurse -Force }
        throw
    }
}
if (-not $online) { $From = Safe-Path $From }
$Prefix = Safe-Path $Prefix
$profileRoot = Safe-Path $env:USERPROFILE
if (-not $Prefix.StartsWith($profileRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Choose an installation prefix inside your user profile; system-wide installation is not supported.' }
$destination = Join-Path $Prefix 'Maris'
if (-not $online -and $From.Equals($destination, [StringComparison]::OrdinalIgnoreCase)) { throw 'Source and destination must differ.' }
function Check-Destination {
    $null = Safe-Path $Prefix
    $null = Safe-Path $destination
    if (Test-Path -LiteralPath $destination) {
        $null = Payload-Files $destination
        Check-Marker $destination
        $binary = Join-Path $destination 'bin\maris.exe'
        # An exclusive write handle fails for an in-use or read-only executable. Never kill it.
        try { $probe = [IO.File]::Open($binary, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        catch { throw 'Installed Maris is in use or not writable. Close it before upgrading.' }
        $probe.Dispose()
    }
}
$stage = ''; $backup = ''; $buildDirectory = ''; $downloadDirectory = ''; $lockPath = ''; $lockHandle = $null; $committed = $false
$exitCode = 0
try {
    if ($online) {
        if ($DryRun) {
            $selection = if ($Version) { "v$Version" } else { 'latest approved stable release' }
            Write-Output "Dry run: download precompiled Windows/$arch $selection from francis-du/maris; verify archive and executable; install $destination"
            Write-Output 'No network, compilation, files or audio changed.'
            return
        }
        $download = Receive-MarisPackage $Version $arch
        $downloadDirectory = $download.Directory
        $From = Safe-Path $download.Payload
    }
    if ($Build) {
        if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'Cargo.lock'))) { throw '-Build requires a source checkout.' }
        if ($DryRun) { Write-Output "Dry run: locked native Windows build; install $destination"; return }
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install Rust 1.94+ and the Visual Studio C++ build tools first.' }
        Push-Location $PSScriptRoot
        try {
            & cargo build --release --locked --target-dir (Join-Path $PSScriptRoot 'target')
            if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed.' }
        } finally { Pop-Location }
        $dist = Safe-Path (Join-Path $PSScriptRoot 'dist')
        $buildDirectory = Join-Path $dist ('.install-source.' + [Guid]::NewGuid().ToString('N'))
        $From = Join-Path $buildDirectory 'Maris'
        $null = [IO.Directory]::CreateDirectory((Join-Path $From 'bin'))
        $null = [IO.Directory]::CreateDirectory((Join-Path $From 'resources'))
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\release\maris.exe') -Destination (Join-Path $From 'bin\maris.exe')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'docs\reference\third-party.md') -Destination (Join-Path $From 'resources\THIRD_PARTY.md')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'third_party\eqmac') -Destination (Join-Path $From 'resources\eqmac') -Recurse
        $text = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Cargo.toml'))
        $version = [regex]::Match($text, '(?m)^version = "([0-9.]+)"\r?$').Groups[1].Value
        [IO.File]::WriteAllText((Join-Path $From '.maris-package'), "maris-package-v1`nwindows`n$arch`n$version`n", ([Text.UTF8Encoding]::new($false)))
    }
    Validate-Payload $From
    Check-Destination
    Write-Output "Source: $From"
    Write-Output "Install: $destination"
    if ($AllowUnsigned) { Write-Warning 'Explicitly accepting trusted local development code, not a signed public release.' }
    if ($DryRun) { Write-Output 'Dry run complete; no files changed.'; return }
    if (-not $Yes) {
        if ([Console]::IsInputRedirected) { throw 'Noninteractive installation requires -Yes.' }
        $answer = Read-Host 'Install without starting audio? [y/N]'
        if ($answer -notmatch '^(y|yes)$') { Write-Output 'Cancelled.'; return }
    }
    $null = [IO.Directory]::CreateDirectory($Prefix)
    $candidateLock = Join-Path $Prefix '.maris-install.lock'
    try { $lockHandle = [IO.File]::Open($candidateLock, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    catch { throw 'Another installation may be active. Inspect the existing lock; never delete a live lock.' }
    $lockPath = $candidateLock
    Check-Destination
    $stage = Join-Path $Prefix ('.maris-stage.' + [Guid]::NewGuid().ToString('N'))
    $null = [IO.Directory]::CreateDirectory($stage)
    $originalDigest = Payload-Digest $From
    Copy-Item -LiteralPath $From -Destination (Join-Path $stage 'Maris') -Recurse
    Validate-Payload (Join-Path $stage 'Maris')
    if ((Payload-Digest (Join-Path $stage 'Maris')) -ne $originalDigest -or (Payload-Digest $From) -ne $originalDigest) { throw 'Payload changed during copy.' }
    Check-Destination
    if ([IO.Directory]::Exists($destination)) {
        $backup = Join-Path $Prefix ('.maris-backup.' + [Guid]::NewGuid().ToString('N'))
        $null = [IO.Directory]::CreateDirectory($backup)
        [IO.Directory]::Move($destination, (Join-Path $backup 'Maris'))
    }
    [IO.Directory]::Move((Join-Path $stage 'Maris'), $destination)
    $committed = $true
    Write-Output "Installed: $destination"
    Write-Output ('Command: & "' + (Join-Path $destination 'bin\maris.exe') + '"')
    if ($backup) { Write-Output "Previous payload retained: $backup\Maris" }
    Write-Output 'Preferences and PATH preserved. No services, drivers, login items or audio were started.'
    Write-Output 'Audio was not started. Launch Maris explicitly when you are ready to process system or application audio.'
} catch {
    [Console]::Error.WriteLine('Maris install: ' + $_.Exception.Message)
    $exitCode = 1
} finally {
    if (-not $committed -and $backup -and [IO.Directory]::Exists((Join-Path $backup 'Maris')) -and -not (Test-Path -LiteralPath $destination)) {
        try { [IO.Directory]::Move((Join-Path $backup 'Maris'), $destination) }
        catch { [Console]::Error.WriteLine("Recovery required: $backup\Maris"); $exitCode = 1 }
    }
    foreach ($owned in @($stage, $buildDirectory, $downloadDirectory)) {
        if ($owned -and [IO.Directory]::Exists($owned)) {
            try { Remove-Item -LiteralPath $owned -Recurse -Force }
            catch { [Console]::Error.WriteLine("Retained temporary directory: $owned") }
        }
    }
    if ($backup -and [IO.Directory]::Exists($backup) -and @(Get-ChildItem -LiteralPath $backup -Force).Count -eq 0) { [IO.Directory]::Delete($backup) }
    if ($null -ne $lockHandle) { $lockHandle.Dispose() }
    if ($lockPath) { [IO.File]::Delete($lockPath) }
}
exit $exitCode
