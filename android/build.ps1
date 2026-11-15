[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$SdkPlatform,
    [Parameter(Mandatory = $true)][string]$BuildTools,
    [Parameter(Mandatory = $true)][string]$Ndk,
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$BundleDirectory = '',
    [string]$XzJar = (Join-Path $PSScriptRoot '../target/android-libs/xz-1.12.jar'),
    [ValidateSet('arm64-v8a', 'x86_64')][string[]]$Abis = @('arm64-v8a', 'x86_64')
)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$bundled = -not [string]::IsNullOrWhiteSpace($BundleDirectory)
$outputName = if ($bundled) { 'target/android-build-loverable' } else { 'target/android-build' }
$output = Join-Path $project $outputName
$delivery = Join-Path $project 'bin'
$platformJar = Join-Path $SdkPlatform 'android.jar'
$ndkBin = Join-Path $Ndk 'toolchains/llvm/prebuilt/windows-x86_64/bin'
foreach ($required in @($platformJar, $XzJar, "$BuildTools/aapt2.exe", "$BuildTools/d8.bat", "$ndkBin/llvm-readelf.exe", "$JavaHome/bin/javac.exe")) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Missing build tool $required" }
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with $LASTEXITCODE" }
}
New-Item -ItemType Directory -Force -Path $output, $delivery, "$output/classes", "$output/dex" | Out-Null
$manifest = Join-Path $PSScriptRoot 'app/src/main/AndroidManifest.xml'
$bundleFiles = @()
if ($bundled) {
    $BundleDirectory = (Resolve-Path -LiteralPath $BundleDirectory).Path
    $bundleJson = Join-Path $BundleDirectory 'bundled-game.json'
    $bundle = Get-Content -LiteralPath $bundleJson -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($bundle.id -ne 'loverable-cn' -or @($bundle.files).Count -eq 0) { throw 'Invalid Loverable bundle manifest' }
    $bundleFiles += [PSCustomObject]@{Source=$bundleJson; Entry='assets/bundled-game.json'; Compression=[IO.Compression.CompressionLevel]::Optimal}
    foreach ($file in $bundle.files) {
        if ($file.asset -notmatch '^bundled/data[0-9]+\.ykc\.xz$' -or $file.encoding -ne 'xz') { throw 'Bundle assets must be XZ-encoded YKC archives' }
        $source = Join-Path $BundleDirectory $file.asset
        if ((Get-Item -LiteralPath $source).Length -ne $file.encodedSize) { throw "Bundle length mismatch $source" }
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $file.encodedSha256) { throw "Bundle checksum mismatch $source" }
        $bundleFiles += [PSCustomObject]@{Source=$source; Entry=('assets/' + $file.asset); Compression=[IO.Compression.CompressionLevel]::NoCompression}
    }
    $payloadBytes = ($bundle.files | Measure-Object -Property encodedSize -Sum).Sum
    if ($payloadBytes -gt 0xffffffffL - 32MB) { throw 'Compressed game exceeds the ZIP32 APK size budget' }
    [xml]$manifestXml = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8
    $manifestXml.manifest.SetAttribute('package', 'org.yukars.loverable')
    $manifestXml.manifest.application.SetAttribute('label', 'http://schemas.android.com/apk/res/android', [string]$bundle.name) | Out-Null
    $manifest = Join-Path $output 'AndroidManifest.xml'
    $manifestXml.Save($manifest)
}
$env:JAVA_HOME = $JavaHome
$env:PATH = "$JavaHome/bin;$env:PATH"
Push-Location $project
try {
    foreach ($abi in ($Abis | Select-Object -Unique)) {
        $triple = if ($abi -eq 'arm64-v8a') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
        $cargoPrefix = 'CARGO_TARGET_' + $triple.Replace('-', '_').ToUpperInvariant()
        Set-Item -Path "Env:${cargoPrefix}_LINKER" -Value "$ndkBin/$($triple)26-clang.cmd"
        Set-Item -Path "Env:${cargoPrefix}_RUSTFLAGS" -Value '-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384 -C link-arg=-Wl,-z,relro -C link-arg=-Wl,-z,now'
        Invoke-Checked 'cargo' @('rustc', '--release', '--lib', '--locked', '--target', $triple, '--crate-type', 'cdylib')
        Invoke-Checked "$ndkBin/llvm-readelf.exe" @('-l', "target/$triple/release/libyuka_rs.so")
    }
    $javaFiles = @(Get-ChildItem -LiteralPath "$PSScriptRoot/app/src/main/java" -Recurse -Filter '*.java' | Select-Object -ExpandProperty FullName)
    Invoke-Checked "$JavaHome/bin/javac.exe" (@('-encoding', 'UTF-8', '--release', '8', '-Xlint:all', '-Werror', '-classpath', "$platformJar;$XzJar", '-d', "$output/classes") + $javaFiles)
    $classJar = [IO.Compression.ZipArchive]::new([IO.File]::Open("$output/classes.jar", [IO.FileMode]::Create), [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($class in (Get-ChildItem -LiteralPath "$output/classes" -Recurse -Filter '*.class')) {
            $entryName = [IO.Path]::GetRelativePath("$output/classes", $class.FullName).Replace('\', '/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($classJar, $class.FullName, $entryName) | Out-Null
        }
    } finally { $classJar.Dispose() }
    # XZ's multi-release JAR also contains Java 9 optimizations. Android uses its Java 8 classes.
    $xzSource = [IO.Compression.ZipFile]::OpenRead($XzJar)
    $xzBaseline = [IO.Compression.ZipArchive]::new([IO.File]::Open("$output/xz-baseline.jar", [IO.FileMode]::Create), [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entry in $xzSource.Entries) {
            if (-not $entry.FullName.StartsWith('org/tukaani/xz/') -or -not $entry.FullName.EndsWith('.class')) { continue }
            $inputStream = $entry.Open()
            $outputStream = $xzBaseline.CreateEntry($entry.FullName).Open()
            try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose(); $inputStream.Dispose() }
        }
    } finally { $xzBaseline.Dispose(); $xzSource.Dispose() }
    Invoke-Checked "$BuildTools/d8.bat" @('--release', '--min-api', '26', '--lib', $platformJar, '--output', "$output/dex", "$output/classes.jar", "$output/xz-baseline.jar")
    Invoke-Checked "$BuildTools/aapt2.exe" @('link', '--manifest', $manifest, '-I', $platformJar, '--min-sdk-version', '26', '--target-sdk-version', '35', '-o', "$output/resources.apk")
    # Create mode streams multi-gigabyte assets directly to disk. Update mode buffers added entries.
    $archive = [IO.Compression.ZipArchive]::new([IO.File]::Open("$output/unsigned.apk", [IO.FileMode]::Create), [IO.Compression.ZipArchiveMode]::Create)
    $resourceArchive = [IO.Compression.ZipFile]::OpenRead("$output/resources.apk")
    try {
        foreach ($entry in $resourceArchive.Entries) {
            $inputStream = $entry.Open()
            $outputStream = $archive.CreateEntry($entry.FullName, [IO.Compression.CompressionLevel]::NoCompression).Open()
            try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose(); $inputStream.Dispose() }
        }
        foreach ($dex in (Get-ChildItem -LiteralPath "$output/dex" -Filter '*.dex')) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $dex.FullName, $dex.Name, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
        }
        foreach ($abi in ($Abis | Select-Object -Unique)) {
            $triple = if ($abi -eq 'arm64-v8a') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, "$project/target/$triple/release/libyuka_rs.so", "lib/$abi/libyuka_rs.so", [IO.Compression.CompressionLevel]::NoCompression) | Out-Null
        }
        foreach ($file in $bundleFiles) {
            Write-Output ("Packaging " + $file.Entry)
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.Source, $file.Entry, $file.Compression) | Out-Null
        }
    } finally { $resourceArchive.Dispose(); $archive.Dispose() }
    if ((Get-Item -LiteralPath "$output/unsigned.apk").Length -gt 0xffffffffL - 1MB) { throw 'Unsigned APK leaves insufficient ZIP32 signing space' }
    $keystore = Join-Path $delivery 'android-development.jks'
    if (-not (Test-Path -LiteralPath $keystore)) {
        Invoke-Checked "$JavaHome/bin/keytool.exe" @('-genkeypair', '-keystore', $keystore, '-storepass', 'android', '-keypass', 'android', '-alias', 'yuka-rs', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000', '-dname', 'CN=Yuka-Rs Development', '-noprompt')
    }
    Invoke-Checked "$BuildTools/zipalign.exe" @('-f', '-P', '16', '4', "$output/unsigned.apk", "$output/aligned.apk")
    $apkName = if ($bundled) { 'Loverable-Yuka-Rs-0.2.1.apk' } else { 'Yuka-Rs-0.2.1.apk' }
    $apk = Join-Path $delivery $apkName
    Invoke-Checked "$BuildTools/apksigner.bat" @('sign', '--ks', $keystore, '--ks-key-alias', 'yuka-rs', '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', $apk, "$output/aligned.apk")
    Invoke-Checked "$BuildTools/apksigner.bat" @('verify', '--verbose', $apk)
    Invoke-Checked "$BuildTools/zipalign.exe" @('-c', '-P', '16', '4', $apk)
    Get-FileHash -LiteralPath $apk -Algorithm SHA256
} finally { Pop-Location }
