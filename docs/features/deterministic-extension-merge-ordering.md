# Deterministic Extension Merge Ordering

## Overview

avocadoctl enforces deterministic systemd-sysext/confext merge ordering based on the runtime manifest's extension array position. The first extension listed in the avocado config has the highest priority -- if the same file exists in multiple extensions, the rootfs sees the version from the first-listed extension.

## Problem

systemd-sysext and systemd-confext determine extension overlay priority by the **lexicographic name** of entries in `/run/extensions/` and `/run/confexts/`. Extensions named `00-base` are lowest priority (bottom of the overlay stack) while `99-override` is highest. Without intervention, merge order is alphabetical by extension name, which is arbitrary and not user-controllable.

Users should not be forced to name their extensions with numerical prefixes (e.g., `00-base`, `01-networking`) because extensions are reusable components across runtimes. The same extension may need different priority in different runtimes.

A second constraint is that systemd requires `extension-release.NAME` inside each extension to match the directory or symlink name in `/run/extensions/`. Renaming a symlink to `02-myapp` causes systemd to look for `extension-release.02-myapp`, but the extension image only contains `extension-release.myapp`.

## Solution: Bind Mount over extension-release.d

The ordering flows through the entire pipeline: **Config -> Manifest -> Merge**.

### Config declares priority

```yaml
runtimes:
  dev:
    extensions:
      - highest-layer    # wins conflicts, top of overlay stack
      - middle-layer
      - lowest-layer     # base layer, bottom of overlay stack
```

### avocadoctl assigns prefixed names at merge time

For each manifest extension at position `i` (of `N` total), avocadoctl:

1. Computes an inverted merge index: `merge_index = N - 1 - i`
2. Generates a prefixed name: `{merge_index:02}-{name}` (e.g., `02-highest-layer`)
3. Creates a staging directory on tmpfs with copies of the original `extension-release.d/` contents plus a new `extension-release.{prefixed-name}` file
4. Bind mounts the staging directory over the extension's real `extension-release.d/` directory
5. Creates a symlink `/run/extensions/{prefixed-name}` pointing to the extension path

This works even on read-only `.raw` mounts because the bind mount shadows the directory entry without modifying the underlying filesystem. No kernel modules, FUSE, or special configuration required -- `mount --bind` is a core Linux VFS feature.

### Result

For the config above (N=3):

```
/run/extensions/
  00-lowest-layer     <- config[2], lowest priority (base layer)
  01-middle-layer     <- config[1], middle priority
  02-highest-layer    <- config[0], highest priority (wins conflicts)
```

systemd's lexicographic ordering now matches the user's declared intent.

## HITL Override Behavior

When a HITL (Hardware-In-The-Loop) extension overrides a manifest extension, the HITL version **inherits the same priority slot** as the manifest extension it replaces. This keeps ordering honest and avoids misleading priority changes.

```
/run/extensions/
  00-lowest-layer     <- config[2], from manifest image
  01-middle-layer     <- config[1], HITL override (same priority slot)
  02-highest-layer    <- config[0], from manifest image
```

HITL extensions that don't correspond to any manifest entry get no prefix (legacy behavior).

## Why not FUSE

FUSE was considered as an alternative to remap `extension-release.NAME` files dynamically. It was rejected because:

- Adds a userspace daemon dependency (daemon crash = extension filesystem unavailable)
- I/O overhead on every file access through the FUSE layer
- Requires a new libfuse dependency
- More code to write and maintain (a full passthrough filesystem)

Bind mounts are kernel-level, zero-overhead, require no daemon, and use standard Linux primitives already used throughout avocadoctl.

## Why not full OverlayFS

OverlayFS would require per-extension upper/work directories and the overlayfs kernel module. Since only a single file needs to be added to `extension-release.d/`, a targeted bind mount is simpler and sufficient.

## Legacy / Backward Compatibility

When no active runtime manifest is present, extensions have no `merge_index` and fall back to the existing behavior: no prefix, alphabetical ordering by systemd. The feature is opt-in based on manifest presence.

## Observability

`avocadoctl ext status` displays an **Order** column showing the assigned prefix (e.g., `#00`, `#01`). The JSON output includes an `"order"` field.

## Cleanup

During `avocadoctl ext unmerge`:

1. systemd-sysext/confext unmerge runs first (removes the overlay)
2. Bind mounts over `extension-release.d/` directories are unmounted (discovered via `/proc/mounts`)
3. Staging directories at `/run/avocado/ext-release-staging/` are removed
4. Symlinks in `/run/extensions/` and `/run/confexts/` are cleaned up
5. Raw loop devices are unmounted (if `--unmount` requested)

## Implementation

All in `src/commands/ext.rs`:

- `Extension` struct -- `merge_index: Option<usize>` field
- `scan_extensions_from_all_sources_with_verbosity()` -- assigns inverted merge indices during manifest scanning; HITL overrides inherit the manifest extension's index
- `compute_prefixed_name()` -- generates `"NN-name"` or plain `"name"` based on merge_index presence
- `stage_extension_release()` -- creates staging dir, copies release files, adds prefixed release file, bind mounts over original
- `run_bind_mount()` -- executes `mount --bind` (skipped in test mode)
- `cleanup_extension_release_staging()` -- unmounts bind mounts and removes staging dirs
- `create_sysext_symlink_with_verbosity()` / `create_confext_symlink_with_verbosity()` -- accept pre-computed prefixed name
- `prepare_extension_environment_with_output()` -- orchestrates staging + symlink creation
- `display_extension_info()` / `build_extension_json_list()` -- Order column and JSON field
