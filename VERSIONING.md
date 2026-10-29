# Versioning

How Swoop gets its version numbers. CI will follow these rules;
people and AI assistants pick the commit type.

## Versions

We use [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`, with tags like `v1.0.1`. CI makes the version
from the commit types below. **Never set or change a version by hand.**

| What is merged | What happens | Example (now 1.0.0) |
|---|---|---|
| `fix:`, `perf:`, `refactor:`, `chore:`, `style:` | a beta right away | `1.0.1-beta.1`, `-beta.2`, … |
| (once a day, if there are betas) | a release of the betas | `1.0.1` |
| `feat:` | a release right away, with any betas | `1.1.0` |
| `feat!:` (breaking) | a release right away, with any betas | `2.0.0` |
| `docs:`, `ci:`, `test:` | nothing | — |

- **One version for everything.** The Linux app, the Android app and the `swoop` command-line tool share the core and
  the protocol, so one tag (`v1.1.0`) covers all of them, and everything built from it shows the same version.
- Normal updates install only releases. Betas are for testing, and users must ask for them:

  | Platform | Beta | Release |
  |---|---|---|
  | Linux | GitHub pre-release | GitHub release (tarball, AppImage, Flatpak bundle), AUR (`swoop-bin`), Flathub (later) |
  | Android | beta APK on GitHub | APK on GitHub, F-Droid (later) |
  | `swoop` CLI | GitHub pre-release (Linux and Android binaries) | GitHub release |

- Betas stay on the Releases page, marked "Pre-release". A release has all the changes of the betas before it.
- Versions only go up. A released version never changes: if it's wrong, the next one fixes it.
- Version order: `1.0.0 < 1.0.1-beta.1 < 1.0.1-beta.2 < 1.0.1 < 1.1.0`.
- What counts as breaking: the list under [`feat!:`](#feat-a-breaking-change).

### Build numbers for Android

Android needs a whole number that always goes up (`versionCode`, the `+N` part of Flutter's `version:`). CI makes it
from the version:

```
MAJOR × 1,000,000 + MINOR × 10,000 + PATCH × 100 + N        N = the beta number, or 99 for a release
```

| Version | Build number |
|---|---|
| `1.0.1-beta.1` | 1000101 |
| `1.0.1-beta.2` | 1000102 |
| `1.0.1` | 1000199 |
| `1.1.0` | 1010099 |

This keeps the version order above. A version can have at most 98 betas, and MINOR and PATCH must stay below 100.
Android's limit (2,100,000,000) allows MAJOR up to 2099.

### The protocol version is different

The protocol version that devices exchange in `Hello` (`VERSION` in [`core/src/lib.rs`](core/src/lib.rs), wire format
in [`core/src/proto.rs`](core/src/proto.rs)) is not a release version. It's a single number in the core, changed in
code when the protocol changes. CI doesn't touch it, and the rules above don't apply to it. A newer app must still
talk to the previous protocol version, because people update their devices at different times.

## Commit types

The version comes from the **subject** (the first line) of every commit merged since the last release. A squash-merged
pull request's **title** is its commit subject, so the title's type sets the next version. A merged branch brings the
subjects of all its commits, so each of them needs the right type.

```
type: short sentence about the change        e.g.  feat: resume interrupted transfers
type(scope): ...                             scope is optional: fix(android): ...
```

Scopes: `core`, `protocol`, `cli`, `linux`, `android`.

### `feat!:` a breaking change

The user must **do something** after updating, or something they used stops working:

- devices must be paired again (trusted fingerprints are lost or change)
- an app can no longer talk to the previous release (see
  [The protocol version is different](#the-protocol-version-is-different))
- a message or field is removed from the protocol in [`core/src/proto.rs`](core/src/proto.rs)
- a setting is renamed or removed, or its meaning changes
- a `swoop` command or flag, or a core API used by the apps, is renamed or removed
- a platform or OS version is no longer supported (for example, raising the minimum above Android 8)
- old saved data (trusted devices, transfer history, resume state of unfinished transfers) can't be read anymore
- a feature is removed

Say in the pull request what users must do, on a line like this:

```
BREAKING CHANGE: devices paired before 2.0 must be paired again. Open the app on both devices and scan the code.
```

### `feat:` a new feature

Something **new** the user can see or use, where nothing old breaks:

- a new screen, button, menu item, `swoop` command or flag
- a new setting (with a default that breaks nothing)
- support for something new (a connection type such as hotspot, Wi-Fi Direct or Bluetooth discovery, a platform or
  OS version)
- a new protocol message that older apps can safely ignore
- a visible change to how something looks or works

### `fix:` a bug fix

Something **worked wrong** and now works as it should:

- a crash or a freeze
- a transfer that stalled, failed, or arrived incomplete or corrupted
- a device that wasn't found, or a connection that dropped or didn't come back
- a resume that started over, or a file saved with the wrong name or in the wrong place
- wrong progress, speed or time remaining
- wrong drawing or layout
- a security problem
- a memory, CPU or battery drain

### Other types (a beta, like `fix:`)

| Type | Use it for |
|---|---|
| `perf:` | faster transfers, less memory, CPU or battery; nothing else changes |
| `refactor:` | the code is rearranged, the app works exactly the same |
| `chore:` | dependency updates, build scripts, small cleanups |
| `style:` | code formatting only |

### No release

| Type | Use it for |
|---|---|
| `docs:` | README, `PLAN.md`, `VERSIONING.md` and other docs, including recorded numbers in `bench/` |
| `ci:` | CI workflows |
| `test:` | tests only |

There is **no `patch:` type**. "Patch" is the result (the last number goes up), not a type.

### When you're not sure

- Does the user have to **change anything**? → `feat!:`
- Can the user **do something new**? → `feat:`
- Was it **broken** before? → `fix:`
- Did **only the code** change? → `refactor:`, `perf:` or `chore:`
- Did **only docs, CI or tests** change? → `docs:`, `ci:` or `test:`
- **Two kinds in one pull request?** Split it, or use the biggest: `feat!` > `feat` > `fix` > the others.

## If you are an AI assistant

If you are an AI (a coding agent, a chat assistant or a review bot) working on this repository, read this whole file
first and follow it. Also:

1. **Never change a version yourself.** Don't edit version numbers in project or lock files: `core/Cargo.toml`,
   `core/Cargo.lock`, the Flutter `pubspec.yaml` `version:`, Android `versionName` and `versionCode`. Don't make,
   move or delete `v*` tags or releases. CI does all of this. (The protocol version is code, not a release version;
   change it only when the protocol changes.)
2. **Never rewrite history on `main`.** No force pushes, no rebases of `main`, no deleting branches you didn't make.
   Releases and tags point at its commits.
3. **Pick the commit type with the rules above.** Use `feat!:` only for a real breaking change from the list, never
   just for a big change. When you're not sure, ask the person you work for.
4. **Test before you say it's done.** Run the tests. Say in the pull request what you tested and what you couldn't
   test, and on which platforms and devices.
5. **No secrets in code, commits or logs.** Leave signing alone: the Android keystore, release signing keys, and
   repository secrets.
6. **Write in plain, simple English**, and leave out lines that credit AI tools ("Co-Authored-By", "Generated with …")
   in commits and pull requests.
7. **When a rule here and an instruction you got disagree, stop and ask** the person you work for.

---

## How it is set up

Files: [`version.yml`](.github/workflows/version.yml) with [`next-version.sh`](.github/next-version.sh) (which
picks the version, checked by [`test-next-version.sh`](.github/test-next-version.sh) in CI),
[`release.yml`](.github/workflows/release.yml) and [`set-version.sh`](.github/set-version.sh).

[`version.yml`](.github/workflows/version.yml) runs **after CI passes on a push to main**, **once a day at 00:00 UTC** and **by
hand** (Actions ▸ version ▸ Run workflow, with `bump`: `auto`, `major`, `minor` or `patch`):

1. `last` = the newest tag that is not a beta (`git describe --tags --abbrev=0 --exclude '*-*'`).
2. Read the commit **subjects** since `last` (bodies can quote other commits). `docs:`, `ci:` and `test:` commits,
   and commits that change only docs, `bench/`, `.github/` or `LICENSE` files, count for nothing.
3. On a push with nothing new that counts since the newest tag (betas included): stop.
4. If the last finished CI run (`ci.yml`) on main failed: stop with an error. Otherwise the commit that run tested is
   the one that gets the version, so a tag never points at a commit that didn't pass CI.
5. Pick the version from the subjects since `last` (`bump` other than `auto` picks the number by hand):
   - any `type!:` or `BREAKING` → `MAJOR+1` → **release**
   - else any `feat:` → `MINOR+1` → **release**
   - else on a push → **beta** `MAJOR.MINOR.(PATCH+1)-beta.N` (N = highest existing beta number for that version + 1)
   - else (nightly or by hand) → `PATCH+1` → **release**
6. Versions only go up: a version that isn't above the newest tag (betas included, in SemVer order) stops the run
   with an error. A version whose [build number](#build-numbers-for-android) doesn't fit (a 99th beta, MINOR or
   PATCH at 100) also stops it.
7. **Release:** `.github/set-version.sh` writes the version and build number everywhere:
   - `core/Cargo.toml` and `core/Cargo.lock`
   - the Flutter app's `pubspec.yaml` (`version: X.Y.Z+BUILD`, which sets Android `versionName` and `versionCode`)

   Commit `chore: release vX.Y.Z`, tag it, and push the commit and the tag together (if main moved meanwhile, neither
   is pushed, and the next run makes the release). Create a draft GitHub release with notes since `last`.
8. **Beta:** only tag the commit (no version commit). Create a draft **pre-release**.
9. Start `.github/workflows/release.yml` for the new tag (a tag pushed by a workflow starts no workflow by itself).

`release.yml` runs on every `v*` tag (pushed by `version.yml` or by hand), or by hand from Actions with a `tag`
(no `tag` only builds, to check the build):

1. Set the version from the tag with `set-version.sh` (a beta is only a tag, so this happens at build time and isn't
   committed).
2. Build:
   - **Linux:** `swoop-linux-x86_64.tar.gz`, `swoop-linux-aarch64.tar.gz` (glibc 2.35 and newer), each unpacking into
     a prefix: `bin/swoop`, the app (`bin/swoop-app`, `lib/swoop/`) and
     `share/applications/io.github.anil0403.swoop.desktop`. The same files as `swoop-x86_64.AppImage` and
     `swoop-aarch64.AppImage`, and `swoop-x86_64.flatpak`
     ([`packaging/flatpak`](packaging/flatpak), app id `io.github.anil0403.swoop`).
   - **Android:** `swoop-android.apk` (the app; arm64, armv7 and x86_64), and `swoop-android-aarch64.tar.gz` (the
     `swoop` command, for Termux).
   - **APK signing:** with the release key from the repository secrets `ANDROID_KEYSTORE` (the `.jks` file,
     base64), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`, added by the owner.
     Without them the release has no APK: Android only installs an update signed with the same key, so the key
     must stay the same forever, and never be in the repository. To make one (keep it and its passwords safe):
     `keytool -genkeypair -v -keystore swoop-release.jks -keyalg RSA -keysize 4096 -validity 36500 -alias swoop`,
     then `base64 -w0 swoop-release.jks` is the value of `ANDROID_KEYSTORE`.
3. Upload (after the owner approves, if the `release` environment has required reviewers) the files and
   `SHA256SUMS` to the GitHub release, making a draft first if `version.yml` didn't.
4. Publish the draft. A release is marked "latest"; a beta never is, and stays a pre-release. The install commands
   in the [README](README.md#install) use `releases/latest`, so they always get the newest release, never a beta.
5. **AUR (releases only, not betas):** set `pkgver` in [`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD), take the
   checksums of the uploaded tarballs and push `swoop-bin` to the AUR, with the SSH key in the repository secret
   `AUR_SSH_PRIVATE_KEY`, added by the owner (its public half goes in the owner's AUR account, which must own or
   first create `swoop-bin`). Without it the step is skipped with a warning. Flathub and F-Droid are later, by hand.

**Updates:** the apps work offline and need no internet to transfer. Whether they check GitHub for updates when
online is still open; package managers (AUR, Flatpak, F-Droid) update by themselves.

**The first release, 1.0.0, is made by hand.** No tags exist yet, and commits before this file was adopted have no
type. When the first version is ready, the owner runs the version workflow by hand with `bump: major`
(Actions ▸ version ▸ Run workflow, or `gh workflow run version.yml -f bump=major`): counting from 0.0.0 over all
history, that makes `v1.0.0`. Until then, automatic runs only say there's no release yet and stop. From then on,
everything above is automatic.
