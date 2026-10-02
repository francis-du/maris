# Load only the installer's checksum helper; never run installation or download code.
param([Parameter(Mandatory=$true)][string]$Fixture)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot '../../install.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'Installer did not parse.' }
$functions = @($ast.FindAll({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -ceq 'File-Sha256'
}, $true))
if ($functions.Count -ne 1) { throw 'Expected exactly one installer checksum helper.' }
. ([ScriptBlock]::Create($functions[0].Extent.Text))
function Get-FileHash { throw 'The installer must not depend on the checksum cmdlet.' }
$cases = ConvertFrom-Json ([IO.File]::ReadAllText((Join-Path $Fixture 'cases.json')))
foreach ($case in $cases) {
    $path = Join-Path $Fixture $case.name
    $actual = File-Sha256 $path
    if ($actual -cne $case.expected) { throw "SHA-256 differs for $($case.name)." }
    # The reader must release its handle, even when files are empty.
    $exclusive = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $exclusive.Dispose()
}
$rejected = $false
try { $null = File-Sha256 (Join-Path $Fixture 'missing-file') } catch { $rejected = $true }
if (-not $rejected) { throw 'A missing file was accepted as a valid checksum.' }
Write-Output 'Checksum vectors and released handles passed.'
