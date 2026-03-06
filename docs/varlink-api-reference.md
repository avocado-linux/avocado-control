# avocadoctl Varlink API Reference

avocadoctl exposes a Varlink IPC interface over a Unix socket at `/run/avocado/avocadoctl.sock`.
All methods are called using JSON messages per the Varlink protocol.

This document uses `sd-varlink` from libsystemd (v258+) for all C examples. It is the recommended
approach as it requires no extra dependencies -- `libsystemd` is already present on every supported
system.

Compile with: `$(pkg-config --cflags --libs libsystemd)`

For a fully type-safe, struct-based C experience, `vali` can generate native C structs from the
`.varlink` IDL files installed at `/usr/share/varlink/interfaces/`.

---

## Connection and Teardown

Every client must connect once and release the connection when done.

```c
#include <systemd/sd-varlink.h>
#include <systemd/sd-json.h>

sd_varlink *vl = NULL;
int r;

r = sd_varlink_connect_address(&vl, "unix:/run/avocado/avocadoctl.sock");
if (r < 0) {
    fprintf(stderr, "Failed to connect: %s\n", strerror(-r));
    return r;
}

/* ... make calls ... */

sd_varlink_unref(vl);
```

---

## Error Handling

All `sd_varlink_call` return values follow the `sd_*` convention: negative errno on failure.
When the server replies with a Varlink error, `sd_varlink_call` returns `-EBADR` and the error
name/payload can be retrieved with `sd_varlink_get_error`.

```c
const char *error_id = NULL;
sd_json_variant *error_params = NULL;

r = sd_varlink_call(vl, "org.avocado.Extensions.Merge", NULL, &reply);
if (r < 0) {
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Call failed: %s (varlink error: %s)\n",
            strerror(-r), error_id ? error_id : "transport error");
}
```

---

## org.avocado.Extensions

Extension management: list, merge, unmerge, enable, disable, and status.

### Types

```varlink
type Extension (
    name: string,
    version: ?string,
    path: string,
    isSysext: bool,
    isConfext: bool,
    isDirectory: bool
)

type ExtensionStatus (
    name: string,
    version: ?string,
    isSysext: bool,
    isConfext: bool,
    isMerged: bool,
    origin: ?string,
    imageId: ?string
)
```

### Errors

| Error | Fields | Description |
|-------|--------|-------------|
| `org.avocado.Extensions.ExtensionNotFound` | `name: string` | Named extension does not exist |
| `org.avocado.Extensions.MergeFailed` | `reason: string` | `systemd-sysext`/`systemd-confext` merge failed |
| `org.avocado.Extensions.UnmergeFailed` | `reason: string` | Unmerge operation failed |
| `org.avocado.Extensions.ConfigurationError` | `message: string` | Invalid configuration |
| `org.avocado.Extensions.CommandFailed` | `command: string`, `message: string` | Underlying system command failed |

---

### List

```varlink
method List() -> (extensions: []Extension)
```

List all available extensions in the extensions directory.

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.Extensions.List", NULL, &reply);
if (r < 0) {
    /* handle error */
    goto cleanup;
}

sd_json_variant *exts = sd_json_variant_by_key(reply, "extensions");
size_t n = sd_json_variant_elements(exts);

for (size_t i = 0; i < n; i++) {
    sd_json_variant *ext = sd_json_variant_by_index(exts, i);

    const char *name    = sd_json_variant_string(sd_json_variant_by_key(ext, "name"));
    const char *version = sd_json_variant_string(sd_json_variant_by_key(ext, "version")); /* may be NULL */
    const char *path    = sd_json_variant_string(sd_json_variant_by_key(ext, "path"));
    bool is_sysext      = sd_json_variant_boolean(sd_json_variant_by_key(ext, "isSysext"));
    bool is_confext     = sd_json_variant_boolean(sd_json_variant_by_key(ext, "isConfext"));
    bool is_dir         = sd_json_variant_boolean(sd_json_variant_by_key(ext, "isDirectory"));

    printf("%-32s  %-12s  %s  sysext=%d confext=%d dir=%d\n",
           name, version ? version : "-", path, is_sysext, is_confext, is_dir);
}

cleanup:
    sd_json_variant_unref(reply);
```

---

### Merge

```varlink
method Merge() -> ()
```

Merge all enabled extensions via `systemd-sysext merge` and `systemd-confext merge`.
Requires the daemon to be running as root.

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.Extensions.Merge", NULL, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    if (error_id && strstr(error_id, "MergeFailed")) {
        sd_json_variant *err_params = NULL;
        sd_varlink_get_error(vl, &error_id); /* already have it */
        /* sd_varlink_get_error_parameters if available, or inspect reply */
        fprintf(stderr, "Merge failed: %s\n", error_id);
    }
    goto cleanup;
}

printf("Extensions merged successfully.\n");

cleanup:
    sd_json_variant_unref(reply);
```

---

### Unmerge

```varlink
method Unmerge(unmount: ?bool) -> ()
```

Unmerge extensions. When `unmount` is `true`, also unmount any loop-mounted extension images.
`unmount` is optional; omit it (pass a JSON object without the key) to use the default behavior.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

/* Build parameters: {"unmount": true} */
r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_BOOLEAN("unmount", true)));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Extensions.Unmerge", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Unmerge failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

printf("Extensions unmerged successfully.\n");

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

To omit the optional `unmount` field, pass `NULL` as params:

```c
r = sd_varlink_call(vl, "org.avocado.Extensions.Unmerge", NULL, &reply);
```

---

### Refresh

```varlink
method Refresh() -> ()
```

Atomically unmerge then re-merge extensions. Equivalent to `Unmerge` followed by `Merge`.

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.Extensions.Refresh", NULL, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Refresh failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

printf("Extensions refreshed.\n");

cleanup:
    sd_json_variant_unref(reply);
```

---

### Enable

```varlink
method Enable(extensions: []string, osRelease: ?string) -> (enabled: int, failed: int)
```

Enable the named extensions for the given OS release. `osRelease` is optional; when omitted
the current OS release is used. Returns counts of successfully enabled and failed extensions.

```c
sd_json_variant *params   = NULL;
sd_json_variant *ext_list = NULL;
sd_json_variant *reply    = NULL;

/* Build the extensions array */
r = sd_json_build(&ext_list,
        SD_JSON_BUILD_ARRAY(
            SD_JSON_BUILD_STRING("my-extension"),
            SD_JSON_BUILD_STRING("another-extension")));
if (r < 0)
    goto cleanup;

/* Build params: {"extensions": [...], "osRelease": "1.2.3"} */
r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR("extensions", SD_JSON_BUILD_VARIANT(ext_list)),
            SD_JSON_BUILD_PAIR_STRING("osRelease", "1.2.3")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Extensions.Enable", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Enable failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

int64_t enabled = sd_json_variant_integer(sd_json_variant_by_key(reply, "enabled"));
int64_t failed  = sd_json_variant_integer(sd_json_variant_by_key(reply, "failed"));
printf("Enabled: %" PRId64 "  Failed: %" PRId64 "\n", enabled, failed);

cleanup:
    sd_json_variant_unref(ext_list);
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Disable

```varlink
method Disable(extensions: ?[]string, all: ?bool, osRelease: ?string) -> (disabled: int, failed: int)
```

Disable extensions. Pass either a list of extension names in `extensions`, or set `all` to `true`
to disable every enabled extension. `osRelease` is optional.

**Disable specific extensions:**

```c
sd_json_variant *params   = NULL;
sd_json_variant *ext_list = NULL;
sd_json_variant *reply    = NULL;

r = sd_json_build(&ext_list,
        SD_JSON_BUILD_ARRAY(SD_JSON_BUILD_STRING("my-extension")));
if (r < 0)
    goto cleanup;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR("extensions", SD_JSON_BUILD_VARIANT(ext_list))));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Extensions.Disable", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Disable failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

int64_t disabled = sd_json_variant_integer(sd_json_variant_by_key(reply, "disabled"));
int64_t failed   = sd_json_variant_integer(sd_json_variant_by_key(reply, "failed"));
printf("Disabled: %" PRId64 "  Failed: %" PRId64 "\n", disabled, failed);

cleanup:
    sd_json_variant_unref(ext_list);
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

**Disable all extensions:**

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_BOOLEAN("all", true)));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Extensions.Disable", params, &reply);
/* ... check r, read reply ... */

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Status

```varlink
method Status() -> ()
```

Show the status of currently merged extensions. This method returns no structured data;
it is intended for informational/side-effect use.

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.Extensions.Status", NULL, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Status failed: %s\n", error_id ? error_id : strerror(-r));
}

sd_json_variant_unref(reply);
```

---

## org.avocado.Runtimes

Runtime lifecycle management: stage, activate, inspect, and remove runtimes.

### Types

```varlink
type RuntimeInfo (
    name: string,
    version: string
)

type ManifestExtension (
    name: string,
    version: string,
    imageId: ?string
)

type Runtime (
    id: string,
    manifestVersion: int,
    builtAt: string,
    runtime: RuntimeInfo,
    extensions: []ManifestExtension,
    active: bool
)
```

### Errors

| Error | Fields | Description |
|-------|--------|-------------|
| `org.avocado.Runtimes.RuntimeNotFound` | `id: string` | No runtime matches the given ID |
| `org.avocado.Runtimes.AmbiguousRuntimeId` | `id: string`, `candidates: []string` | ID prefix matches more than one runtime |
| `org.avocado.Runtimes.RemoveActiveRuntime` | _(none)_ | Attempted to remove the currently active runtime |
| `org.avocado.Runtimes.StagingFailed` | `reason: string` | Staging a new runtime failed |
| `org.avocado.Runtimes.UpdateFailed` | `reason: string` | Activating a runtime failed |

---

### List

```varlink
method List() -> (runtimes: []Runtime)
```

List all staged runtimes, including the active one.

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.Runtimes.List", NULL, &reply);
if (r < 0)
    goto cleanup;

sd_json_variant *runtimes = sd_json_variant_by_key(reply, "runtimes");
size_t n = sd_json_variant_elements(runtimes);

for (size_t i = 0; i < n; i++) {
    sd_json_variant *rt = sd_json_variant_by_index(runtimes, i);

    const char *id      = sd_json_variant_string(sd_json_variant_by_key(rt, "id"));
    const char *built   = sd_json_variant_string(sd_json_variant_by_key(rt, "builtAt"));
    bool active         = sd_json_variant_boolean(sd_json_variant_by_key(rt, "active"));

    sd_json_variant *info    = sd_json_variant_by_key(rt, "runtime");
    const char *rt_name      = sd_json_variant_string(sd_json_variant_by_key(info, "name"));
    const char *rt_version   = sd_json_variant_string(sd_json_variant_by_key(info, "version"));

    printf("[%s] %s  %s@%s  built=%s\n",
           active ? "ACTIVE" : "      ", id, rt_name, rt_version, built);
}

cleanup:
    sd_json_variant_unref(reply);
```

---

### AddFromUrl

```varlink
method AddFromUrl(url: string) -> ()
```

Stage a new runtime by fetching its manifest and extension images from a TUF repository URL.
The URL must point to a TUF repository root.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("url", "https://updates.example.com/tuf")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Runtimes.AddFromUrl", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    if (error_id && strstr(error_id, "StagingFailed")) {
        /* Inspect error_id for details; server includes a "reason" field */
        fprintf(stderr, "Staging failed: %s\n", error_id);
    } else {
        fprintf(stderr, "AddFromUrl error: %s\n", strerror(-r));
    }
    goto cleanup;
}

printf("Runtime staged successfully.\n");

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### AddFromManifest

```varlink
method AddFromManifest(manifestPath: string) -> ()
```

Stage a new runtime from a local manifest JSON file. Useful for offline provisioning or testing.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("manifestPath", "/var/lib/avocado/manifests/runtime-v2.json")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Runtimes.AddFromManifest", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "AddFromManifest failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

printf("Runtime staged from manifest.\n");

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Remove

```varlink
method Remove(id: string) -> ()
```

Remove a staged (non-active) runtime. `id` may be a full runtime ID or a unique prefix.
Returns `RemoveActiveRuntime` if the ID matches the currently active runtime.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("id", "a3f8c1")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Runtimes.Remove", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    if (error_id) {
        if (strstr(error_id, "RuntimeNotFound"))
            fprintf(stderr, "No such runtime: a3f8c1\n");
        else if (strstr(error_id, "AmbiguousRuntimeId"))
            fprintf(stderr, "Prefix matches multiple runtimes; use a longer prefix.\n");
        else if (strstr(error_id, "RemoveActiveRuntime"))
            fprintf(stderr, "Cannot remove the active runtime.\n");
        else
            fprintf(stderr, "Remove failed: %s\n", error_id);
    }
    goto cleanup;
}

printf("Runtime removed.\n");

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Activate

```varlink
method Activate(id: string) -> ()
```

Activate a staged runtime. This installs the runtime's extension images and updates the active
marker. `id` may be a full ID or a unique prefix.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("id", "a3f8c1")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Runtimes.Activate", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    if (error_id && strstr(error_id, "UpdateFailed"))
        fprintf(stderr, "Activation failed: %s\n", error_id);
    else
        fprintf(stderr, "Activate error: %s\n", strerror(-r));
    goto cleanup;
}

printf("Runtime activated.\n");

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Inspect

```varlink
method Inspect(id: string) -> (runtime: Runtime)
```

Return detailed information about a specific runtime. `id` may be a full ID or a unique prefix.

```c
sd_json_variant *params = NULL;
sd_json_variant *reply  = NULL;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("id", "a3f8c1")));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Runtimes.Inspect", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "Inspect failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

sd_json_variant *rt      = sd_json_variant_by_key(reply, "runtime");
const char *id           = sd_json_variant_string(sd_json_variant_by_key(rt, "id"));
const char *built        = sd_json_variant_string(sd_json_variant_by_key(rt, "builtAt"));
bool active              = sd_json_variant_boolean(sd_json_variant_by_key(rt, "active"));
sd_json_variant *info    = sd_json_variant_by_key(rt, "runtime");
const char *rt_name      = sd_json_variant_string(sd_json_variant_by_key(info, "name"));
const char *rt_version   = sd_json_variant_string(sd_json_variant_by_key(info, "version"));

printf("ID:      %s\n", id);
printf("Runtime: %s %s\n", rt_name, rt_version);
printf("Built:   %s\n", built);
printf("Active:  %s\n", active ? "yes" : "no");

sd_json_variant *exts = sd_json_variant_by_key(rt, "extensions");
size_t n = sd_json_variant_elements(exts);
printf("Extensions (%zu):\n", n);
for (size_t i = 0; i < n; i++) {
    sd_json_variant *ext      = sd_json_variant_by_index(exts, i);
    const char *ext_name      = sd_json_variant_string(sd_json_variant_by_key(ext, "name"));
    const char *ext_version   = sd_json_variant_string(sd_json_variant_by_key(ext, "version"));
    printf("  %s@%s\n", ext_name, ext_version);
}

cleanup:
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

## org.avocado.Hitl

Hardware-in-the-loop testing: mount and unmount NFS-exported extension images from a remote server.

### Errors

| Error | Fields | Description |
|-------|--------|-------------|
| `org.avocado.Hitl.MountFailed` | `extension: string`, `reason: string` | NFS mount for the named extension failed |
| `org.avocado.Hitl.UnmountFailed` | `extension: string`, `reason: string` | Unmount of the named extension failed |

---

### Mount

```varlink
method Mount(serverIp: string, serverPort: ?string, extensions: []string) -> ()
```

Mount NFS extension images from a remote HITL server. `serverPort` is optional and defaults
to the standard NFS port when omitted.

```c
sd_json_variant *params   = NULL;
sd_json_variant *ext_list = NULL;
sd_json_variant *reply    = NULL;

r = sd_json_build(&ext_list,
        SD_JSON_BUILD_ARRAY(
            SD_JSON_BUILD_STRING("test-extension-a"),
            SD_JSON_BUILD_STRING("test-extension-b")));
if (r < 0)
    goto cleanup;

/* With optional serverPort */
r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR_STRING("serverIp", "192.168.10.1"),
            SD_JSON_BUILD_PAIR_STRING("serverPort", "2049"),
            SD_JSON_BUILD_PAIR("extensions", SD_JSON_BUILD_VARIANT(ext_list))));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Hitl.Mount", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "HITL mount failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

printf("HITL extensions mounted.\n");

cleanup:
    sd_json_variant_unref(ext_list);
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

### Unmount

```varlink
method Unmount(extensions: []string) -> ()
```

Unmount NFS extension images previously mounted via `Mount`.

```c
sd_json_variant *params   = NULL;
sd_json_variant *ext_list = NULL;
sd_json_variant *reply    = NULL;

r = sd_json_build(&ext_list,
        SD_JSON_BUILD_ARRAY(
            SD_JSON_BUILD_STRING("test-extension-a"),
            SD_JSON_BUILD_STRING("test-extension-b")));
if (r < 0)
    goto cleanup;

r = sd_json_build(&params,
        SD_JSON_BUILD_OBJECT(
            SD_JSON_BUILD_PAIR("extensions", SD_JSON_BUILD_VARIANT(ext_list))));
if (r < 0)
    goto cleanup;

r = sd_varlink_call(vl, "org.avocado.Hitl.Unmount", params, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    fprintf(stderr, "HITL unmount failed: %s\n", error_id ? error_id : strerror(-r));
    goto cleanup;
}

printf("HITL extensions unmounted.\n");

cleanup:
    sd_json_variant_unref(ext_list);
    sd_json_variant_unref(params);
    sd_json_variant_unref(reply);
```

---

## org.avocado.RootAuthority

Trust anchor information: inspect the TUF signing keys trusted on this device.

### Types

```varlink
type TrustedKey (
    keyId: string,
    keyType: string,
    roles: []string
)

type RootAuthorityInfo (
    version: int,
    expires: string,
    keys: []TrustedKey
)
```

### Errors

| Error | Fields | Description |
|-------|--------|-------------|
| `org.avocado.RootAuthority.NoRootAuthority` | _(none)_ | No root authority file is present on this device |
| `org.avocado.RootAuthority.ParseFailed` | `reason: string` | Root authority file exists but could not be parsed |

---

### Show

```varlink
method Show() -> (authority: ?RootAuthorityInfo)
```

Return the trust anchor for this device. The `authority` field is null when no root authority
has been provisioned (distinct from the `NoRootAuthority` error, which is returned when the
authority file is expected but missing).

```c
sd_json_variant *reply = NULL;

r = sd_varlink_call(vl, "org.avocado.RootAuthority.Show", NULL, &reply);
if (r < 0) {
    const char *error_id = NULL;
    sd_varlink_get_error(vl, &error_id);
    if (error_id && strstr(error_id, "NoRootAuthority"))
        fprintf(stderr, "No root authority provisioned on this device.\n");
    else if (error_id && strstr(error_id, "ParseFailed"))
        fprintf(stderr, "Root authority file is corrupt.\n");
    else
        fprintf(stderr, "Show failed: %s\n", strerror(-r));
    goto cleanup;
}

sd_json_variant *authority = sd_json_variant_by_key(reply, "authority");
if (sd_json_variant_is_null(authority)) {
    printf("No root authority.\n");
    goto cleanup;
}

int64_t version   = sd_json_variant_integer(sd_json_variant_by_key(authority, "version"));
const char *expires = sd_json_variant_string(sd_json_variant_by_key(authority, "expires"));
printf("Root Authority v%" PRId64 "  (expires: %s)\n", version, expires);

sd_json_variant *keys = sd_json_variant_by_key(authority, "keys");
size_t n = sd_json_variant_elements(keys);
printf("Trusted keys (%zu):\n", n);
for (size_t i = 0; i < n; i++) {
    sd_json_variant *key     = sd_json_variant_by_index(keys, i);
    const char *key_id       = sd_json_variant_string(sd_json_variant_by_key(key, "keyId"));
    const char *key_type     = sd_json_variant_string(sd_json_variant_by_key(key, "keyType"));

    sd_json_variant *roles   = sd_json_variant_by_key(key, "roles");
    size_t nr = sd_json_variant_elements(roles);

    printf("  %s  (%s)  roles:", key_id, key_type);
    for (size_t j = 0; j < nr; j++)
        printf(" %s", sd_json_variant_string(sd_json_variant_by_index(roles, j)));
    printf("\n");
}

cleanup:
    sd_json_variant_unref(reply);
```

---

## Quick Reference

| Method | Parameters | Returns |
|--------|-----------|---------|
| `org.avocado.Extensions.List` | _(none)_ | `extensions: []Extension` |
| `org.avocado.Extensions.Merge` | _(none)_ | _(none)_ |
| `org.avocado.Extensions.Unmerge` | `unmount: ?bool` | _(none)_ |
| `org.avocado.Extensions.Refresh` | _(none)_ | _(none)_ |
| `org.avocado.Extensions.Enable` | `extensions: []string`, `osRelease: ?string` | `enabled: int`, `failed: int` |
| `org.avocado.Extensions.Disable` | `extensions: ?[]string`, `all: ?bool`, `osRelease: ?string` | `disabled: int`, `failed: int` |
| `org.avocado.Extensions.Status` | _(none)_ | _(none)_ |
| `org.avocado.Runtimes.List` | _(none)_ | `runtimes: []Runtime` |
| `org.avocado.Runtimes.AddFromUrl` | `url: string` | _(none)_ |
| `org.avocado.Runtimes.AddFromManifest` | `manifestPath: string` | _(none)_ |
| `org.avocado.Runtimes.Remove` | `id: string` | _(none)_ |
| `org.avocado.Runtimes.Activate` | `id: string` | _(none)_ |
| `org.avocado.Runtimes.Inspect` | `id: string` | `runtime: Runtime` |
| `org.avocado.Hitl.Mount` | `serverIp: string`, `serverPort: ?string`, `extensions: []string` | _(none)_ |
| `org.avocado.Hitl.Unmount` | `extensions: []string` | _(none)_ |
| `org.avocado.RootAuthority.Show` | _(none)_ | `authority: ?RootAuthorityInfo` |

## Testing without Code

Use `varlinkctl` to exercise the API interactively:

```bash
# Introspect available interfaces
varlinkctl info /run/avocado/avocadoctl.sock
varlinkctl introspect /run/avocado/avocadoctl.sock org.avocado.Extensions

# Call methods
varlinkctl call /run/avocado/avocadoctl.sock org.avocado.Extensions.List '{}'
varlinkctl call /run/avocado/avocadoctl.sock org.avocado.Extensions.Merge '{}'
varlinkctl call /run/avocado/avocadoctl.sock org.avocado.Extensions.Enable \
    '{"extensions": ["my-extension"]}'
varlinkctl call /run/avocado/avocadoctl.sock org.avocado.Runtimes.Inspect \
    '{"id": "a3f8c1"}'
varlinkctl call /run/avocado/avocadoctl.sock org.avocado.RootAuthority.Show '{}'
```
