# SPDX-License-Identifier: GPL-3.0-only
# Shared, read-only tool discovery. Dot-sourcing this file never creates directories.

function Resolve-YukaToolPath {
    param(
        [string]$Name,
        [string]$Explicit,
        [string]$EnvironmentName,
        [string[]]$Candidates,
        [string]$Probe = '',
        [switch]$Optional
    )
    $configured = if (-not [string]::IsNullOrWhiteSpace($Explicit)) { $Explicit }
        elseif ($EnvironmentName) { [Environment]::GetEnvironmentVariable($EnvironmentName) }
        else { '' }
    $paths = if (-not [string]::IsNullOrWhiteSpace($configured)) { @($configured) } else { $Candidates }
    foreach ($candidate in $paths) {
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        $path = $candidate
        if (-not $Probe -and -not (Test-Path -LiteralPath $path -PathType Leaf)) {
            $command = Get-Command -Name $path -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($command) { $path = $command.Source }
        }
        $required = if ($Probe) { Join-Path $path $Probe } else { $path }
        if (Test-Path -LiteralPath $required -PathType Leaf) { return (Resolve-Path -LiteralPath $path).ProviderPath }
    }
    if ($Optional -and [string]::IsNullOrWhiteSpace($configured)) { return '' }
    $hint = if ($configured) { "Configured path '$configured' is unavailable" } else { 'No usable installation was found' }
    throw "$Name`: $hint. Pass an explicit path or set $EnvironmentName."
}

function Get-YukaSdkPackages {
    param([string[]]$Roots, [string]$Section, [string]$Prefix = '', [string]$Probe)
    foreach ($root in $Roots) {
        $directory = Join-Path $root $Section
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) { continue }
        Get-ChildItem -LiteralPath $directory -Directory | Where-Object {
            $version = [version]'0.0'
            if ($Prefix -and -not $_.Name.StartsWith($Prefix)) { return $false }
            $number = if ($Prefix -and $_.Name.StartsWith($Prefix)) { $_.Name.Substring($Prefix.Length) } else { $_.Name }
            if ($number -match '^\d+$') { $number += '.0' }
            [version]::TryParse($number, [ref]$version) -and (Test-Path -LiteralPath (Join-Path $_.FullName $Probe) -PathType Leaf)
        } | Sort-Object -Property @{ Expression = {
            $number = $_.Name.Substring($Prefix.Length)
            if ($number -match '^\d+$') { $number += '.0' }
            [version]$number
        }; Descending = $true } |
            Select-Object -ExpandProperty FullName
    }
}

function Resolve-YukaAndroidTools {
    param(
        [string]$Project,
        [string]$SdkRoot = '',
        [string]$SdkPlatform = '',
        [string]$BuildTools = '',
        [string]$Ndk = '',
        [string]$JavaHome = '',
        [string]$XzJar = '',
        [string]$Adb = '',
        [switch]$RequireNdk,
        [switch]$RequireAdb
    )
    $selectedSdk = if ($SdkRoot) { $SdkRoot } else { $env:YUKA_ANDROID_SDK_ROOT }
    if ($selectedSdk -and -not (Test-Path -LiteralPath $selectedSdk -PathType Container)) {
        throw "Configured Android SDK root is unavailable: $selectedSdk"
    }
    # The shared F: installation is reused as-is. Missing emulator/images/adb are
    # not inferred from source.properties: every selected tool has a real probe.
    $roots = @($selectedSdk, $env:ANDROID_SDK_ROOT, $env:ANDROID_HOME,
        'F:\ethornell-android\sdk', (Join-Path $Project 'target/android-sdk')) |
        Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) } | Select-Object -Unique
    $platforms = @(Get-YukaSdkPackages $roots 'platforms' 'android-' 'android.jar') +
        @((Join-Path $Project 'target/android-sdk/android-35'))
    $builders = @(Get-YukaSdkPackages $roots 'build-tools' '' 'aapt2.exe') +
        @((Join-Path $Project 'target/android-sdk/android-15'))
    $ndks = @(Get-YukaSdkPackages $roots 'ndk' '' 'toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-readelf.exe') +
        @((Join-Path $Project 'target/android-sdk/android-ndk-r27c'))
    $adbCandidates = @('E:\platform-tools\adb.exe') +
        @($roots | ForEach-Object { Join-Path $_ 'platform-tools/adb.exe' }) + @('adb')
    $javac = Get-Command javac -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    $javaCandidates = @($env:JAVA_HOME, 'E:\DevEco Studio\jbr', 'D:\DevEco Studio\jbr')
    if ($javac) { $javaCandidates += Split-Path -Parent (Split-Path -Parent $javac.Source) }
    $result = [PSCustomObject]@{
        SdkPlatform = Resolve-YukaToolPath 'Android platform' $SdkPlatform 'YUKA_ANDROID_PLATFORM' $platforms 'android.jar'
        BuildTools = Resolve-YukaToolPath 'Android build tools' $BuildTools 'YUKA_ANDROID_BUILD_TOOLS' $builders 'aapt2.exe'
        Ndk = Resolve-YukaToolPath 'Android NDK' $Ndk 'YUKA_ANDROID_NDK' $ndks 'toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-readelf.exe' -Optional:(-not $RequireNdk)
        JavaHome = Resolve-YukaToolPath 'Java development kit' $JavaHome 'YUKA_JAVA_HOME' $javaCandidates 'bin/javac.exe'
        XzJar = Resolve-YukaToolPath 'XZ for Java 1.12' $XzJar 'YUKA_XZ_JAR' @((Join-Path $Project 'target/android-libs/xz-1.12.jar'))
        Adb = Resolve-YukaToolPath 'Android platform tools' $Adb 'YUKA_ADB' $adbCandidates -Optional:(-not $RequireAdb)
    }
    $required = @('aapt2.exe', 'd8.bat', 'apksigner.bat', 'zipalign.exe') | ForEach-Object { Join-Path $result.BuildTools $_ }
    $required += @('bin/javac.exe', 'bin/keytool.exe') | ForEach-Object { Join-Path $result.JavaHome $_ }
    if ($RequireNdk) {
        $required += @('llvm-readelf.exe', 'aarch64-linux-android26-clang.cmd', 'x86_64-linux-android26-clang.cmd') |
            ForEach-Object { Join-Path $result.Ndk ('toolchains/llvm/prebuilt/windows-x86_64/bin/' + $_) }
    }
    foreach ($file in $required) {
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Incomplete tool installation: missing $file" }
    }
    return $result
}

function Get-YukaAndroidWorkRoot {
    param([string]$Project)
    if ($env:YUKA_ANDROID_WORK) { return [IO.Path]::GetFullPath($env:YUKA_ANDROID_WORK, $Project) }
    $digest = [Security.Cryptography.SHA256]::Create()
    try { $hash = $digest.ComputeHash([Text.Encoding]::UTF8.GetBytes($Project.ToLowerInvariant())) }
    finally { $digest.Dispose() }
    $identifier = [BitConverter]::ToString($hash).Replace('-', '').Substring(0, 12).ToLowerInvariant()
    return Join-Path ([IO.Path]::GetTempPath()) ("yuka-rs-android-" + $identifier)
}

function Get-YukaCargoTargetDirectory {
    param([string]$Project, [string]$Explicit = '')
    $directory = if ($Explicit) { $Explicit } elseif ($env:YUKA_ANDROID_CARGO_TARGET) { $env:YUKA_ANDROID_CARGO_TARGET }
        elseif ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    return [IO.Path]::GetFullPath($directory, $Project)
}

function Remove-YukaIntermediateApks {
    param([string]$Directory)
    $parent = [IO.Path]::GetFullPath($Directory).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    foreach ($name in @('unsigned.apk', 'aligned.apk')) {
        $artifact = [IO.Path]::GetFullPath((Join-Path $parent $name))
        if ([IO.Path]::GetDirectoryName($artifact) -ne $parent) { throw 'Unexpected intermediate APK path' }
        if (Test-Path -LiteralPath $artifact -PathType Leaf) { Remove-Item -LiteralPath $artifact }
    }
}

function Remove-YukaV4Sidecar {
    param([string]$Apk)
    # apksigner with v4 disabled leaves an older sidecar in place on rebuild.
    # It no longer describes the verified APK and must not reach adb install.
    $sidecar = [IO.Path]::GetFullPath($Apk) + '.idsig'
    if (Test-Path -LiteralPath $sidecar -PathType Leaf) { Remove-Item -LiteralPath $sidecar }
}
