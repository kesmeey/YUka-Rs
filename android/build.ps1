[CmdletBinding()]
param(
    [string]$SdkRoot = '',
    [string]$SdkPlatform = '',
    [string]$BuildTools = '',
    [string]$Ndk = '',
    [string]$JavaHome = '',
    [string]$BundleDirectory = '',
    [string]$IconExecutable = '',
    [string]$OutputDirectory = '',
    [string]$ApkOutputDirectory = '',
    [string]$CargoTargetDirectory = '',
    [string]$XzJar = '',
    [ValidateSet('arm64-v8a', 'x86_64')][string[]]$Abis = @('arm64-v8a', 'x86_64'),
    [switch]$ThinUpdate,
    [switch]$CheckTools,
    [switch]$KeepIntermediateApks
)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'toolchains.ps1')
$tools = Resolve-YukaAndroidTools -Project $project -SdkRoot $SdkRoot -SdkPlatform $SdkPlatform -BuildTools $BuildTools -Ndk $Ndk -JavaHome $JavaHome -XzJar $XzJar -RequireNdk
$SdkPlatform = $tools.SdkPlatform
$BuildTools = $tools.BuildTools
$Ndk = $tools.Ndk
$JavaHome = $tools.JavaHome
$XzJar = $tools.XzJar
$cargoTarget = Get-YukaCargoTargetDirectory $project $CargoTargetDirectory
$bundled = -not [string]::IsNullOrWhiteSpace($BundleDirectory)
if ($ThinUpdate -and -not $bundled) { throw '-ThinUpdate requires -BundleDirectory' }
$apkPrefix = 'Yuka-Rs'
$outputName = 'package-player'
if ($bundled) {
    $BundleDirectory = (Resolve-Path -LiteralPath $BundleDirectory).Path
    $bundleJson = Join-Path $BundleDirectory 'bundled-game.json'
    $bundle = Get-Content -LiteralPath $bundleJson -Raw -Encoding UTF8 | ConvertFrom-Json
    switch -CaseSensitive ($bundle.id) {
        'loverable-cn' {
            $applicationId = 'org.yukars.loverable'
            $apkPrefix = 'Loverable-Yuka-Rs'
            $outputName = 'package-loverable'
        }
        'dousei-cn' {
            $applicationId = 'org.yukars.dousei'
            $apkPrefix = 'Dousei-Loverable-Yuka-Rs'
            $outputName = 'package-dousei'
        }
        default { throw 'Unsupported game bundle profile' }
    }
    if (@($bundle.files).Count -eq 0 -or [string]::IsNullOrWhiteSpace($bundle.name)) {
        throw 'Invalid game bundle manifest'
    }
}
$output = if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { Join-Path (Get-YukaAndroidWorkRoot $project) $outputName } else { [IO.Path]::GetFullPath($OutputDirectory, $project) }
if ($CheckTools) {
    $tools | Add-Member -NotePropertyName OutputDirectory -NotePropertyValue $output
    $tools | Add-Member -NotePropertyName CargoTargetDirectory -NotePropertyValue $cargoTarget
    $tools
    return
}
$signingDirectory = Join-Path $project 'bin'
$delivery = if ([string]::IsNullOrWhiteSpace($ApkOutputDirectory)) { $signingDirectory }
    else { [IO.Path]::GetFullPath($ApkOutputDirectory, $project) }
$platformJar = Join-Path $SdkPlatform 'android.jar'
$ndkBin = Join-Path $Ndk 'toolchains/llvm/prebuilt/windows-x86_64/bin'
foreach ($required in @($platformJar, $XzJar, "$BuildTools/aapt2.exe", "$BuildTools/d8.bat", "$ndkBin/llvm-readelf.exe", "$JavaHome/bin/javac.exe")) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Missing build tool $required" }
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with $LASTEXITCODE" }
}
New-Item -ItemType Directory -Force -Path $output, $delivery, $signingDirectory, "$output/classes", "$output/dex" | Out-Null
$manifest = Join-Path $PSScriptRoot 'app/src/main/AndroidManifest.xml'
[xml]$versionManifest = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8
$versionName = $versionManifest.manifest.GetAttribute('versionName', 'http://schemas.android.com/apk/res/android')
if ($versionName -notmatch '^\d+\.\d+\.\d+(?:[-.][A-Za-z0-9]+)*$') { throw 'Invalid Android versionName for APK filename' }
$resourceZip = Join-Path $output 'compiled-resources.zip'
Invoke-Checked "$BuildTools/aapt2.exe" @('compile', '--dir', "$PSScriptRoot/app/src/main/res", '-o', $resourceZip)
$resourceArguments = @($resourceZip)
if (-not [string]::IsNullOrWhiteSpace($IconExecutable)) {
    $IconExecutable = (Resolve-Path -LiteralPath $IconExecutable).Path
    $iconResources = Join-Path $output 'icon-res'
    Invoke-Checked 'python' @("$PSScriptRoot/extract-icon.py", $IconExecutable, $iconResources)
    $iconZip = Join-Path $output 'compiled-icon.zip'
    Invoke-Checked "$BuildTools/aapt2.exe" @('compile', '--dir', $iconResources, '-o', $iconZip)
    $resourceArguments += $iconZip
}
$bundleFiles = @()
$bundleFiles += [PSCustomObject]@{Source=(Join-Path $project 'THIRD_PARTY_NOTICES.md'); Entry='assets/licenses/THIRD_PARTY_NOTICES.md'; Compression=[IO.Compression.CompressionLevel]::Optimal}
foreach ($notice in (Get-ChildItem -LiteralPath "$PSScriptRoot/licenses" -File)) {
    $bundleFiles += [PSCustomObject]@{Source=$notice.FullName; Entry=('assets/licenses/' + $notice.Name); Compression=[IO.Compression.CompressionLevel]::Optimal}
}
if ($bundled) {
    if ($ThinUpdate) {
        # Only this generated copy changes; the full bundle remains reusable.
        $bundle | Add-Member -NotePropertyName requiresPreparedResources -NotePropertyValue $true -Force
        $bundleJson = Join-Path $output 'bundled-game-update.json'
        $bundle | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $bundleJson -Encoding utf8NoBOM
    } elseif ($bundle.requiresPreparedResources) {
        throw 'A full APK requires a full bundle descriptor, not a thin-update descriptor'
    }
    $bundleFiles += [PSCustomObject]@{Source=$bundleJson; Entry='assets/bundled-game.json'; Compression=[IO.Compression.CompressionLevel]::Optimal}
    foreach ($file in $bundle.files) {
        if ($file.asset -notmatch '^bundled/data[0-9]+\.ykc\.xz$' -or $file.encoding -ne 'xz') { throw 'Bundle assets must be XZ-encoded YKC archives' }
        if ($ThinUpdate) { continue }
        $source = Join-Path $BundleDirectory $file.asset
        if ((Get-Item -LiteralPath $source).Length -ne $file.encodedSize) { throw "Bundle length mismatch $source" }
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $file.encodedSha256) { throw "Bundle checksum mismatch $source" }
        $bundleFiles += [PSCustomObject]@{Source=$source; Entry=('assets/' + $file.asset); Compression=[IO.Compression.CompressionLevel]::NoCompression}
    }
    $movieBytes = 0L
    $movieManifestPath = Join-Path $BundleDirectory 'movies.json'
    if (Test-Path -LiteralPath $movieManifestPath) {
        $movieManifest = Get-Content -LiteralPath $movieManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($movieManifest.version -ne 1 -or @($movieManifest.movies).Count -gt 100) { throw 'Invalid movie manifest' }
        $movieNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $bundleFiles += [PSCustomObject]@{Source=$movieManifestPath; Entry='assets/movies.json'; Compression=[IO.Compression.CompressionLevel]::Optimal}
        foreach ($movie in $movieManifest.movies) {
            if ($movie.asset -notmatch '^movies/[A-Za-z0-9_-]+\.mp4$' -or $movie.encodedSize -le 0 -or $movie.encodedSha256 -notmatch '^[0-9a-f]{64}$' -or $movie.sourceSha256 -notmatch '^[0-9a-f]{64}$' -or -not $movieNames.Add($movie.asset)) {
                throw 'Movie assets must have unique MP4 paths and verified source/output hashes'
            }
            $movieSource = Join-Path $BundleDirectory $movie.asset
            if ((Get-Item -LiteralPath $movieSource).Length -ne $movie.encodedSize) { throw "Movie length mismatch $movieSource" }
            if ((Get-FileHash -LiteralPath $movieSource -Algorithm SHA256).Hash -ne $movie.encodedSha256) { throw "Movie checksum mismatch $movieSource" }
            # STORED entries let MediaPlayer seek the APK directly via AssetManager.openFd.
            $bundleFiles += [PSCustomObject]@{Source=$movieSource; Entry=('assets/' + $movie.asset); Compression=[IO.Compression.CompressionLevel]::NoCompression}
            $movieBytes += [long]$movie.encodedSize
        }
    }
    $payloadBytes = $movieBytes
    if (-not $ThinUpdate) { $payloadBytes += ($bundle.files | Measure-Object -Property encodedSize -Sum).Sum }
    if ($payloadBytes -gt 0xffffffffL - 32MB) { throw 'Compressed game exceeds the ZIP32 APK size budget' }
    [xml]$manifestXml = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8
    $manifestXml.manifest.SetAttribute('package', $applicationId)
    $manifestXml.manifest.application.SetAttribute('label', 'http://schemas.android.com/apk/res/android', [string]$bundle.name) | Out-Null
    $manifest = Join-Path $output 'AndroidManifest.xml'
    $manifestXml.Save($manifest)
}
if (-not [string]::IsNullOrWhiteSpace($IconExecutable)) {
    [xml]$manifestXml = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8
    $manifestXml.manifest.application.SetAttribute('icon', 'http://schemas.android.com/apk/res/android', '@drawable/ic_game_icon') | Out-Null
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
        Invoke-Checked 'cargo' @('rustc', '--release', '--lib', '--locked', '--target-dir', $cargoTarget, '--target', $triple, '--crate-type', 'cdylib')
        Invoke-Checked "$ndkBin/llvm-readelf.exe" @('-l', "$cargoTarget/$triple/release/libyuka_rs.so")
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
    Invoke-Checked "$BuildTools/aapt2.exe" (@('link', '--manifest', $manifest, '-I', $platformJar, '--min-sdk-version', '26', '--target-sdk-version', '35', '-o', "$output/resources.apk") + $resourceArguments)
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
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, "$cargoTarget/$triple/release/libyuka_rs.so", "lib/$abi/libyuka_rs.so", [IO.Compression.CompressionLevel]::NoCompression) | Out-Null
        }
        foreach ($file in $bundleFiles) {
            Write-Output ("Packaging " + $file.Entry)
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.Source, $file.Entry, $file.Compression) | Out-Null
        }
    } finally { $resourceArchive.Dispose(); $archive.Dispose() }
    if ((Get-Item -LiteralPath "$output/unsigned.apk").Length -gt 0xffffffffL - 1MB) { throw 'Unsigned APK leaves insufficient ZIP32 signing space' }
    $keystore = Join-Path $signingDirectory 'android-development.jks'
    if (-not (Test-Path -LiteralPath $keystore)) {
        Invoke-Checked "$JavaHome/bin/keytool.exe" @('-genkeypair', '-keystore', $keystore, '-storepass', 'android', '-keypass', 'android', '-alias', 'yuka-rs', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000', '-dname', 'CN=Yuka-Rs Development', '-noprompt')
    }
    Invoke-Checked "$BuildTools/zipalign.exe" @('-f', '-P', '16', '4', "$output/unsigned.apk", "$output/aligned.apk")
    $apkName = if ($ThinUpdate) { "$apkPrefix-$versionName-update.apk" }
        else { "$apkPrefix-$versionName.apk" }
    $apk = Join-Path $delivery $apkName
    Invoke-Checked "$BuildTools/apksigner.bat" @('sign', '--v4-signing-enabled', 'false', '--ks', $keystore, '--ks-key-alias', 'yuka-rs', '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', $apk, "$output/aligned.apk")
    Invoke-Checked "$BuildTools/apksigner.bat" @('verify', '--verbose', $apk)
    Invoke-Checked "$BuildTools/zipalign.exe" @('-c', '-P', '16', '4', $apk)
    Remove-YukaV4Sidecar $apk
    Get-FileHash -LiteralPath $apk -Algorithm SHA256
    if (-not $KeepIntermediateApks) { Remove-YukaIntermediateApks $output }
} finally { Pop-Location }
