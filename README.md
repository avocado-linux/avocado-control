# avocadoctl

A CLI tool for managing Avocado Linux system extensions and hardware-in-the-loop (HITL) testing.

## Overview

`avocadoctl` is included in Avocado Linux distribution images and is designed to run within the Avocado Linux runtime environment. It is not typically installed or used outside of Avocado Linux systems.

## Usage

### Extension Management

```bash
# Merge system extensions
avocadoctl merge

# Unmerge system extensions
avocadoctl unmerge

# Refresh extensions (unmerge then merge)
avocadoctl refresh

# Show extension status
avocadoctl status
```

### Hardware-in-the-Loop (HITL) Testing

```bash
# Mount extensions from NFS server for testing
avocadoctl hitl mount -s <server-ip> -e <extension-name>
```

### Global Options

```bash
# Enable verbose output
avocadoctl --verbose <command>

# Use custom config file
avocadoctl --config /path/to/config.toml <command>
```

### Legacy Commands

The original `ext` subcommand syntax is still supported:

```bash
avocadoctl ext merge    # Same as: avocadoctl merge
avocadoctl ext unmerge  # Same as: avocadoctl unmerge
avocadoctl ext refresh  # Same as: avocadoctl refresh
avocadoctl ext status   # Same as: avocadoctl status
```

## Environment

This tool is designed for Avocado Linux and requires:
- systemd-sysext and systemd-confext
- Appropriate filesystem permissions for extension management
- NFS client support (for HITL operations)

For more information, see `avocadoctl --help`.
