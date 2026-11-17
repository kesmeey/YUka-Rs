[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Apk,
    [Parameter(Mandatory = $true)][string]$SdkPlatform,
    [Parameter(Mandatory = $true)][string]$BuildTools,
    [Parameter(Mandatory = $true)][string]$Game,
    [Parameter(Mandatory = $true)][string]$Save,
    [Parameter(Mandatory = $true)][string]$Audio,
    [string]$OriginalSave = '',
    [string]$FullProfile = '',
    [string]$Adb = 'adb',
    [string]$Serial = '',
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$Keystore = '',
    [string]$TargetPackage = 'org.yukars.player',
    [string]$XzJar = (Join-Path $PSScriptRoot '../target/android-libs/xz-1.12.jar'),
    [ValidateSet('SmokeInstrumentation', 'AudioFocusInstrumentation', 'BundledGameInstrumentation', 'OriginalUiInstrumentation', 'PlaybackInstrumentation', 'ActivityControlsInstrumentation', 'SettingsInstrumentation', 'ExtrasInstrumentation', 'MovieInstrumentation')][string]$Runner = 'SmokeInstrumentation',
    [switch]$BuildOnly
)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$output = Join-Path $project ('target/android-tests/run-' + [Guid]::NewGuid().ToString('N'))
$platformJar = Join-Path $SdkPlatform 'android.jar'
if (-not $Keystore) { $Keystore = Join-Path $project 'bin/android-development.jks' }
foreach ($required in @($Apk, $platformJar, $Keystore, $XzJar, "$BuildTools/aapt2.exe", "$BuildTools/d8.bat", "$JavaHome/bin/javac.exe")) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Missing test input $required" }
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with $LASTEXITCODE" }
}
function Quote-AndroidShell([string]$Value) {
    $quote = [string][char]39
    $escapedQuote = $quote + '"' + $quote + '"' + $quote
    return $quote + $Value.Replace($quote, $escapedQuote) + $quote
}
function Write-ClassJar([string]$ClassDirectory, [string]$Destination) {
    $archive = [IO.Compression.ZipArchive]::new([IO.File]::Open($Destination, [IO.FileMode]::Create), [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($class in (Get-ChildItem -LiteralPath $ClassDirectory -Recurse -Filter '*.class')) {
            $entryName = [IO.Path]::GetRelativePath($ClassDirectory, $class.FullName).Replace('\', '/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $class.FullName, $entryName) | Out-Null
        }
    } finally { $archive.Dispose() }
}
New-Item -ItemType Directory -Force -Path "$output/api", "$output/classes", "$output/dex" | Out-Null
$env:JAVA_HOME = $JavaHome
$env:PATH = "$JavaHome/bin;$env:PATH"
$apiSources = @(Get-ChildItem -LiteralPath "$PSScriptRoot/app/src/main/java" -Recurse -Filter '*.java' | Select-Object -ExpandProperty FullName)
$testSources = @(Get-ChildItem -LiteralPath "$PSScriptRoot/tests/src" -Recurse -Filter '*.java' | Select-Object -ExpandProperty FullName)
Invoke-Checked "$JavaHome/bin/javac.exe" (@('-encoding', 'UTF-8', '--release', '8', '-Xlint:all', '-Werror', '-classpath', "$platformJar;$XzJar", '-d', "$output/api") + $apiSources)
Write-ClassJar "$output/api" "$output/api.jar"
Invoke-Checked "$JavaHome/bin/javac.exe" (@('-encoding', 'UTF-8', '--release', '8', '-Xlint:all', '-Werror', '-classpath', "$platformJar;$XzJar;$output/api", '-d', "$output/classes") + $testSources)
Write-ClassJar "$output/classes" "$output/tests.jar"
Invoke-Checked "$BuildTools/d8.bat" @('--release', '--min-api', '26', '--lib', $platformJar, '--classpath', "$output/api.jar", '--classpath', $XzJar, '--output', "$output/dex", "$output/tests.jar")
Invoke-Checked "$BuildTools/aapt2.exe" @('link', '--manifest', "$PSScriptRoot/tests/AndroidManifest.xml", '--rename-instrumentation-target-package', $TargetPackage, '-I', $platformJar, '--min-sdk-version', '26', '--target-sdk-version', '35', '-o', "$output/unsigned.apk")
$archive = [IO.Compression.ZipFile]::Open("$output/unsigned.apk", [IO.Compression.ZipArchiveMode]::Update)
try {
    foreach ($dex in (Get-ChildItem -LiteralPath "$output/dex" -Filter '*.dex')) {
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $dex.FullName, $dex.Name, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
} finally { $archive.Dispose() }
Invoke-Checked "$BuildTools/zipalign.exe" @('-f', '4', "$output/unsigned.apk", "$output/aligned.apk")
$testApk = Join-Path $project 'bin/Yuka-Rs-tests.apk'
Invoke-Checked "$BuildTools/apksigner.bat" @('sign', '--ks', $Keystore, '--ks-key-alias', 'yuka-rs', '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', $testApk, "$output/aligned.apk")
Invoke-Checked "$BuildTools/apksigner.bat" @('verify', '--verbose', $testApk)
Get-FileHash -LiteralPath $testApk -Algorithm SHA256
if ($BuildOnly) { return }
$device = @()
if ($Serial) { $device = @('-s', $Serial) }
Invoke-Checked $Adb ($device + @('install', '-r', $Apk))
Invoke-Checked $Adb ($device + @('install', '-r', $testApk))
$instrument = @('shell', 'am', 'instrument', '-w', '-e', 'game', (Quote-AndroidShell $Game), '-e', 'save', (Quote-AndroidShell $Save), '-e', 'audio', (Quote-AndroidShell $Audio))
if ($OriginalSave) { $instrument += @('-e', 'originalSave', (Quote-AndroidShell $OriginalSave)) }
if ($FullProfile) { $instrument += @('-e', 'fullProfile', (Quote-AndroidShell $FullProfile)) }
$instrument += "org.yukars.player.tests/org.yukars.player.$Runner"
$result = & $Adb @device @instrument 2>&1
$exitCode = $LASTEXITCODE
$result | Write-Output
$result | Set-Content -LiteralPath "$output/instrumentation.txt" -Encoding UTF8
if ($exitCode -ne 0 -or -not ($result -match 'INSTRUMENTATION_CODE: -1')) {
    throw "Android instrumentation failed. See $output/instrumentation.txt"
}
