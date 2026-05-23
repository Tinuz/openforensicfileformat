# OFFF Go SDK (Minimal Profile)

## Doel
De Go SDK implementeert het minimale OFFF API-profiel voor:
- container openen
- manifest lezen
- container/chunk verificatie
- offset naar chunk mapping
- file-index lezen
- analysis resultaten schrijven
- provenance events appenden

## Vereisten
- Go 1.23+

## Installatie
```bash
go mod tidy
```

## Snelle start
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

De smoke test gebruikt `tests/samples/4orensics.case2.offf` als aanwezig. Als deze sample lokaal ontbreekt, wordt de test overgeslagen.
