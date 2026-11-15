[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$SdkPlatform,
    [Parameter(Mandatory = $true)][string]$BuildTools,
    [Parameter(Mandatory = $true)][string]$Ndk,
    [string]$JavaHome = $env:JAVA_HOME,
    [ValidateSet('arm64-v8a', 'x86_64')][string[]]$Abis = @('arm64-v8a', 'x86_64')
)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$output = Join-Path $project 'target/android-build'
$delivery = Join-Path $project 'bin'
$platformJar = Join-Path $SdkPlatform 'android.jar'
$ndkBin = Join-Path $Ndk 'toolchains/llvm/prebuilt/windows-x86_64/bin'
foreach ($required in @($platformJar, "$BuildTools/aapt2.exe", "$BuildTools/d8.bat", "$ndkBin/llvm-readelf.exe", "$JavaHome/bin/javac.exe")) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Missing build tool $required" }
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program exited with $LASTEXITCODE" }
}
New-Item -ItemType Directory -Force -Path $output, $delivery, "$output/classes", "$output/dex" | Out-Null
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
    Invoke-Checked "$JavaHome/bin/javac.exe" (@('-encoding', 'UTF-8', '--release', '8', '-classpath', $platformJar, '-d', "$output/classes") + $javaFiles)
    $classJar = [IO.Compression.ZipArchive]::new([IO.File]::Open("$output/classes.jar", [IO.FileMode]::Create), [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($class in (Get-ChildItem -LiteralPath "$output/classes" -Recurse -Filter '*.class')) {
            $entryName = [IO.Path]::GetRelativePath("$output/classes", $class.FullName).Replace('\', '/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($classJar, $class.FullName, $entryName) | Out-Null
        }
    } finally { $classJar.Dispose() }
    Invoke-Checked "$BuildTools/d8.bat" @('--release', '--min-api', '26', '--lib', $platformJar, '--output', "$output/dex", "$output/classes.jar")
    Invoke-Checked "$BuildTools/aapt2.exe" @('link', '--manifest', "$PSScriptRoot/app/src/main/AndroidManifest.xml", '-I', $platformJar, '--min-sdk-version', '26', '--target-sdk-version', '35', '-o', "$output/unsigned.apk")
    $archive = [IO.Compression.ZipFile]::Open("$output/unsigned.apk", [IO.Compression.ZipArchiveMode]::Update)
    try {
        foreach ($dex in (Get-ChildItem -LiteralPath "$output/dex" -Filter '*.dex')) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $dex.FullName, $dex.Name, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
        }
        foreach ($abi in ($Abis | Select-Object -Unique)) {
            $triple = if ($abi -eq 'arm64-v8a') { 'aarch64-linux-android' } else { 'x86_64-linux-android' }
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, "$project/target/$triple/release/libyuka_rs.so", "lib/$abi/libyuka_rs.so", [IO.Compression.CompressionLevel]::NoCompression) | Out-Null
        }
    } finally { $archive.Dispose() }
    $keystore = Join-Path $delivery 'android-development.jks'
    if (-not (Test-Path -LiteralPath $keystore)) {
        Invoke-Checked "$JavaHome/bin/keytool.exe" @('-genkeypair', '-keystore', $keystore, '-storepass', 'android', '-keypass', 'android', '-alias', 'yuka-rs', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000', '-dname', 'CN=Yuka-Rs Development', '-noprompt')
    }
    Invoke-Checked "$BuildTools/zipalign.exe" @('-f', '-P', '16', '4', "$output/unsigned.apk", "$output/aligned.apk")
    $apk = Join-Path $delivery 'Yuka-Rs-0.2.0.apk'
    Invoke-Checked "$BuildTools/apksigner.bat" @('sign', '--ks', $keystore, '--ks-key-alias', 'yuka-rs', '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', $apk, "$output/aligned.apk")
    Invoke-Checked "$BuildTools/apksigner.bat" @('verify', '--verbose', $apk)
    Invoke-Checked "$BuildTools/zipalign.exe" @('-c', '-P', '16', '4', $apk)
    Get-FileHash -LiteralPath $apk -Algorithm SHA256
} finally { Pop-Location }
