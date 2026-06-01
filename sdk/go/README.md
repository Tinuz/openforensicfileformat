# OFFF Go SDK (Minimal Profile)

## Purpose
The Go SDK implements the minimal OFFF API profile for:
- opening containers
- reading manifests
- verifying containers/chunks
- mapping offsets to chunks
- reading file index rows
- writing analysis outputs
- appending provenance events

## Requirements
- Go 1.23+

## Installation
```bash
go mod tidy
```

## Quick Start
```go
package main

import (
    "fmt"
    offfsdk "github.com/Tinuz/openforensicfileformat/sdk/go"
)

func main() {
    c, err := offfsdk.OpenContainer("../../tests/samples/4orensics.case2.offf")
    if err != nil {
        panic(err)
    }

    manifest, _ := offfsdk.ReadManifest(c)
    fmt.Println("container:", manifest["container_id"])

    result, _ := offfsdk.VerifyContainer(c)
    fmt.Println("valid:", result["valid"])
}
```

## Tests
```bash
go test ./...
```

The smoke test uses `tests/samples/4orensics.case2.offf` when available. If that sample is missing locally, the test is skipped.
