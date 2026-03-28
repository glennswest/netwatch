# Netwatch Pod — Volume Mount Contamination

## Problem

The `infra/netwatch` pod has a foreign volume mount on `/data` that is shadowing the correct netwatch data volume. This is causing the netwatch database to be empty/inaccessible.

## Evidence from Pod Logs

At `2026-03-28 07:46:13`, during the latest redeploy, two mounts were added to the same mountlist `infra_netwatch_netwatch`:

```
07:46:05  container mount added: dst=/data list=infra_netwatch_netwatch src=/raid1/volumes/infra_netwatch_netwatch/data    ← CORRECT
07:46:13  container mount added: dst=/data list=infra_netwatch_netwatch src=/raid1/volumes/pvc/infra_infra-dns-data        ← WRONG
```

The second mount (`infra_infra-dns-data`) belongs to the DNS pod, not netwatch. It shadows the correct netwatch data volume at `/data`, so netwatch opens an empty or wrong database.

## Additional Issue — Missing Log Directory

Stormlog is failing to write netwatch process logs:

```
"failed to open log file","error":"No such file or directory (os error 2)","path":"/var/stormd/logs/netwatch.log"
```

The `/var/stormd/logs/` directory doesn't exist inside the container. This means netwatch process output (stdout/stderr) isn't being captured to file, making debugging harder.

## What Needs to Happen

1. **Remove the stale DNS mount** from the `infra_netwatch_netwatch` mountlist — the `infra_infra-dns-data` PVC should not be mounted on this pod
2. **Restart the netwatch pod** so it picks up only the correct `/data` volume
3. **Verify** that `/data/netwatch.redb` exists after restart (this is the netwatch database)

## Pod Details

- **Namespace:** infra
- **Pod:** netwatch
- **Container ID:** *27AC
- **Pod IP:** 192.168.200.8
- **Image:** registry.gt.lo:5000/netwatch:edge
