# Netwatch Pod — Volume Mount Contamination (RESOLVED)

**Status:** Fixed on 2026-03-28
**Root cause:** mkube bug + missing volume definition in netwatch pod spec

## Problem

The `infra/netwatch` pod had a foreign volume mount on `/data` that shadowed the correct netwatch data volume, causing the netwatch database to be empty/inaccessible.

## Evidence from Pod Logs

At `2026-03-28 07:46:13`, during redeploy, two mounts were added to the same mountlist `infra_netwatch_netwatch`:

```
07:46:05  container mount added: dst=/data list=infra_netwatch_netwatch src=/raid1/volumes/infra_netwatch_netwatch/data    <- CORRECT
07:46:13  container mount added: dst=/data list=infra_netwatch_netwatch src=/raid1/volumes/pvc/infra_infra-dns-data        <- WRONG
```

The second mount (`infra_infra-dns-data`) belongs to the DNS pod, not netwatch.

## Root Cause Analysis

**Two bugs combined to cause this:**

### Bug 1: mkube `fixOrphanedVolumeMounts()` (the real bug)

In `pkg/provider/pvc.go`, the migration function `fixOrphanedVolumeMounts()` was designed to fix DNS pods that had `data` volumeMounts but no matching Volume definition. However, it **hardcoded** the PVC claim name:

```go
if vm.Name == "data" && vm.MountPath == "/data" {
    claimName := pod.Namespace + "-dns-data"  // Always "infra-dns-data"!
```

This meant ANY pod in the `infra` namespace with a `data` volumeMount at `/data` — including netwatch — would get the DNS pod's PVC (`infra-dns-data`) injected as its volume source.

### Bug 2: netwatch `deploy/pod.yaml` (the trigger)

The netwatch pod spec had a `volumeMount` but no corresponding `volumes:` section:

```yaml
spec:
  containers:
    - name: netwatch
      volumeMounts:
        - name: data
          mountPath: /data
  # Missing: volumes: section with PVC definition
```

Without a Volume definition, the mount was "orphaned" and triggered mkube's migration code, which incorrectly assigned `infra-dns-data`.

### The Contamination Sequence

1. netwatch pod loaded from NATS store (or created from pod.yaml)
2. `fixOrphanedVolumeMounts()` detects orphaned `data` mount (no Volume definition)
3. Function adds Volume with `claimName: infra-dns-data` (hardcoded DNS name)
4. Modified pod persisted back to NATS
5. On next reconcile/redeploy, mkube creates mount entry for `/data` pointing to the DNS PVC directory `/raid1/volumes/pvc/infra_infra-dns-data`
6. netwatch opens the DNS pod's database instead of its own

## Fixes Applied

### Fix 1: mkube — pod-specific PVC names (commit e0c2320)

Changed `fixOrphanedVolumeMounts()` to derive PVC name from the pod:
- DNS pods (`*-dns`, `*microdns*`): `{namespace}-dns-data` (backward compat)
- Registry pods (`registry-*`): `{pod}-data`
- All others: `{pod}-data`

### Fix 2: netwatch — explicit volume definition (commit 8c5297b)

Added proper `volumes:` section to `deploy/pod.yaml`:
```yaml
  volumes:
    - name: data
      persistentVolumeClaim:
        claimName: netwatch-data
```

### Fix 3: Redeployment

1. Deployed fixed mkube (verified running commit e0c2320)
2. Deleted contaminated netwatch pod
3. Recreated from fixed pod.yaml
4. Verified: PVC `netwatch-data` is Bound, netwatch UI responding, 3 devices discovered

## What Netwatch Did Wrong

1. **Missing Volume definition** — The pod spec declared a `volumeMount` for `data` at `/data` but had no `volumes:` section defining what backs it. This is incomplete and triggers mkube's orphan migration code.

2. **Reliance on implicit behavior** — By not declaring a PVC, netwatch was depending on mkube to create an ephemeral volume. This is fragile because migration code can override it.

**The correct pattern for any pod needing persistent data:**
```yaml
spec:
  containers:
    - volumeMounts:
        - name: data
          mountPath: /data
  volumes:
    - name: data
      persistentVolumeClaim:
        claimName: {pod-name}-data
```

## Additional Issue — Missing Log Directory

Stormlog failing to write to `/var/stormd/logs/netwatch.log` — the directory doesn't exist in the container. This is a separate issue in the stormdbase image or Containerfile (needs `mkdir -p /var/stormd/logs` or stormd should create it).
