# Object ID Recipe

This document defines deterministic IDs used by filesystem-to-object graph materialization.

## Filesystem file object ID

Recipe:

```text
obj-fsfile- + sha256(
  container_id + "|" +
  filesystem_id + "|" +
  file_id + "|" +
  normalized_logical_path + "|" +
  size_bytes
)[0:24]
```

Normalization:

- path separator is `/`
- logical path starts with `/`

## Edge ID

Recipe:

```text
edge- + sha256(parent_object_id + "|" + child_object_id + "|contains")[0:24]
```

## Derivation ID

Recipe:

```text
drv- + sha256(parent_object_id + "|" + child_object_id + "|filesystem_index_materialization")[0:24]
```

## Properties

- stable across reruns for unchanged input
- independent of iteration order
- deterministic across platforms for identical canonical input strings
