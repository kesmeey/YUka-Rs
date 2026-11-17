# Shared Android tools and build storage

`android/build.ps1` and `android/test.ps1` use `android/toolchains.ps1` to find
already installed tools. Discovery only reads paths; it never downloads an SDK,
changes another project's SDK, creates junctions, moves caches, or deletes an
installation. Run scripts in PowerShell 7.

```powershell
.\android\build.ps1 -CheckTools
```

Explicit component arguments take precedence, followed by the corresponding
`YUKA_*` variable, SDK installations, and the old project-local fallback.
`-SdkRoot` / `YUKA_ANDROID_SDK_ROOT` selects an SDK root; normal
`ANDROID_SDK_ROOT` / `ANDROID_HOME` roots are also considered. The shared root
`F:\ethornell-android\sdk` is checked before the local `target/android-sdk`.
Each chosen tool is checked for its executable/JAR, not merely package metadata.

The verified shared components on this computer are:

| Component | Shared location |
| --- | --- |
| Platform | `F:\ethornell-android\sdk\platforms\android-36` |
| Build tools | `F:\ethornell-android\sdk\build-tools\36.0.0` |
| NDK | `F:\ethornell-android\sdk\ndk\29.0.14206865` |
| Java | `E:\DevEco Studio\jbr` (already outside this repository) |
| ADB | `E:\platform-tools\adb.exe` (already outside this repository) |

The F: emulator, platform-tools and system-image directories contain package
metadata but lack some actual binaries/images. They are not valid replacements
for a working emulator or image. The 168,792-byte XZ 1.12 JAR remains in
`target/android-libs`; it can also be supplied explicitly.

The shared platform/NDK are newer than the previous API35/r27c copies. The
application still targets API35 with minimum API26 and retains 16KiB ELF/APK
alignment. A successful build is required before removing old working tools.

## Overrides

| Argument | Environment variable |
| --- | --- |
| `-SdkPlatform` | `YUKA_ANDROID_PLATFORM` |
| `-BuildTools` | `YUKA_ANDROID_BUILD_TOOLS` |
| `-Ndk` | `YUKA_ANDROID_NDK` |
| `-JavaHome` | `YUKA_JAVA_HOME` (then `JAVA_HOME`) |
| `-XzJar` | `YUKA_XZ_JAR` |
| Test `-Adb` | `YUKA_ADB` |
| `-CargoTargetDirectory` | `YUKA_ANDROID_CARGO_TARGET` (then `CARGO_TARGET_DIR`) |

An explicitly configured missing tool is an error; it is not silently replaced.
Existing calls passing API35/build-tools35/NDKr27c remain supported while those
paths exist. Without a Cargo override, Rust continues to use the project's
`target` path, including an existing user-created junction to another drive.
All `.so` packaging paths use that selected Cargo directory.

On this machine, the project's ignored `target` directory is now a junction to
`F:\llvm-agent-galgame-work\yuka-rs-cache\target`. It contains the retained
compressed game bundle, emulator/image and reusable build caches. Source, the
signing key and deliverable APKs remain on E:. The five duplicate archives in
`evidence/native-pc-compat` are file links to the unchanged original game on D:;
the unique PC saves and investigation logs were retained. The private test AVD's
generated 12GB data overlay and RAM snapshot, redundant local SDK, and disposable
build outputs were removed. Do not remove the original game while relying on
those evidence links.

## Intermediate files and final artifacts

Packaging and test runs default to a per-project directory under the operating
system's temporary directory, rather than adding multi-gigabyte unsigned/aligned
APK copies to this repository. Set `YUKA_ANDROID_WORK` for another work root, or
pass `-OutputDirectory` for one exact run. For example:

```powershell
$env:YUKA_ANDROID_WORK = 'F:\llvm-agent-galgame-work\yuka-rs-cache\android-work'
$env:YUKA_ANDROID_CARGO_TARGET = 'F:\llvm-agent-galgame-work\yuka-rs-cache\target'
.\android\build.ps1 -CheckTools
```

The scripts do not create these directories during `-CheckTools`. Building into
an external directory requires the normal filesystem permission for that path.
Never point a work directory at original game inputs.

After signing, signature verification and alignment checks succeed, the scripts
remove only the generated `unsigned.apk` and `aligned.apk`. Use
`-KeepIntermediateApks` to retain these for debugging. A failed build leaves its
intermediates for diagnosis. V4 `.idsig` sidecars are disabled, and any stale
sidecar for the newly verified output APK is removed. APK v2/v3 signing continues
to use `bin/android-development.jks`.

Final APKs and the signing key stay in `bin/`. `-ApkOutputDirectory` can redirect
a validation build's APK without generating a different signing key or
overwriting the current deliverable. APK names derive from `versionName` in
the Android manifest.

```powershell
.\android\build.ps1 -ApkOutputDirectory "$env:TEMP\yuka-toolchain-validation\apks"
.\android\test.ps1 -Apk .\bin\Yuka-Rs-0.2.5.apk -Game '/private/test/game' `
    -Save '/private/test/save' -Audio '/private/test/audio' -BuildOnly
```

The last command only builds a test APK; it does not install anything or touch a
connected phone. Normal test execution requires explicit game/save/audio paths
and a nonempty `-Serial` to target the intended device. A missing serial fails
before any ADB command; the script never implicitly chooses a connected phone.

## Small updates for an existing bundled installation

Use `-ThinUpdate` with `-BundleDirectory` when the phone already has all five
verified, expanded YKC archives from a full Loverable APK. This builds
`Loverable-Yuka-Rs-<version>-update.apk` with the same application ID and signing
key. It keeps the movie manifest and MP4 assets, while omitting the five large
XZ payloads. The descriptor packaged in this APK sets
`requiresPreparedResources=true`; the source bundle descriptor stays unchanged.

```powershell
.\android\build.ps1 -BundleDirectory .\target\bundled-loverable\assets -ThinUpdate
```

This update requires the existing prepared-resource receipt and matching archive
sizes. It cannot prepare resources for a first install or after app data is
cleared. Use the full APK to prepare those resources first.
