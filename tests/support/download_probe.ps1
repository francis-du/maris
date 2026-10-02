# Native PowerShell production-function regression; transport is isolated, never networked.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Fixture, [Parameter(Mandatory=$true)][string]$Architecture)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'install.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw ('PowerShell syntax errors: ' + ($errors -join '; ')) }
$names = @('Safe-Path','Read-MarisManifest','Expand-MarisArchive','Receive-MarisPackage')
foreach ($name in $names) {
    $functions = @($ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq $name }, $true))
    if ($functions.Count -ne 1) { throw "Missing or ambiguous production function: $name" }
    . ([ScriptBlock]::Create($functions[0].Extent.Text))
}
function Receive-ReleaseFile([string]$Url, [string]$Output, [long]$Limit) {
    if (-not $Url.StartsWith('https://github.com/francis-du/maris/releases/', [StringComparison]::Ordinal)) { throw 'Unexpected request host.' }
    $source = if ($Url.EndsWith('/maris-release.tsv')) { Join-Path $Fixture 'manifest' } else {
        if (-not $Url.Contains('/download/v1.2.3/')) { throw 'Package request was not version-pinned.' }
        Join-Path $Fixture 'archive'
    }
    if ((Get-Item -LiteralPath $source).Length -gt $Limit) { throw 'Fixture size limit exceeded.' }
    Copy-Item -LiteralPath $source -Destination $Output
}
$text = [IO.File]::ReadAllText((Join-Path $Fixture 'manifest'))
$asset = Read-MarisManifest $text $Architecture ''
if ($asset.Version -cne '1.2.3') { throw 'Wrong selected version.' }
foreach ($invalid in @($text.Replace("channel`tstable", "channel`tcandidate"), $text.Replace('source_sha256', 'unexpected'), $text + "extra`n")) {
    $rejected = $false
    try { $null = Read-MarisManifest $invalid $Architecture '' } catch { $rejected = $true }
    if (-not $rejected) { throw 'Malformed manifest was accepted.' }
}
$download = Receive-MarisPackage '' $Architecture
try {
    if (-not [IO.File]::Exists((Join-Path $download.Payload 'bin\maris.exe'))) { throw 'Verified payload was not extracted.' }
    if ([IO.Directory]::Exists((Join-Path $Fixture 'Programs'))) { throw 'Acquisition wrote an installation destination.' }
} finally { Remove-Item -LiteralPath $download.Directory -Recurse -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($badName in @('../escape',('Maris-1.2.3-windows-' + $Architecture + '/../../escape'),('Maris-1.2.3-windows-' + $Architecture + '/CON'))) {
    $zipPath = Join-Path $Fixture ([Guid]::NewGuid().ToString('N') + '.zip')
    $zip = [IO.Compression.ZipFile]::Open($zipPath, [IO.Compression.ZipArchiveMode]::Create)
    try { $entry=$zip.CreateEntry($badName); $writer=[IO.StreamWriter]::new($entry.Open()); $writer.Write('fixture'); $writer.Dispose() }
    finally { $zip.Dispose() }
    $rejected = $false
    try { Expand-MarisArchive $zipPath (Join-Path $Fixture 'unsafe-extract') ('Maris-1.2.3-windows-' + $Architecture) } catch { $rejected=$true }
    if (-not $rejected) { throw 'Unsafe ZIP path was accepted.' }
}
Write-Output 'offline_download_paths_passed; no network, native executable or audio activation'
