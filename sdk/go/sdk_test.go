package offfsdk

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSmokeAgainstSampleContainer(t *testing.T) {
	sample := filepath.Join("..", "..", "tests", "samples", "4orensics.case2.offf")
	if _, err := os.Stat(sample); err != nil {
		t.Skip("sample OFFF container not present")
	}

	c, err := OpenContainer(sample)
	if err != nil {
		t.Fatalf("OpenContainer failed: %v", err)
	}

	manifest, err := ReadManifest(c)
	if err != nil {
		t.Fatalf("ReadManifest failed: %v", err)
	}
	if manifest["container_id"] == nil {
		t.Fatalf("container_id missing in manifest")
	}

	res, err := VerifyContainer(c)
	if err != nil {
		t.Fatalf("VerifyContainer failed: %v", err)
	}
	if _, ok := res["valid"]; !ok {
		t.Fatalf("VerifyContainer missing valid field")
	}

	if len(c.chunks) == 0 {
		t.Fatalf("chunk map is empty")
	}
	first := c.chunks[0]
	if _, err := ReadChunk(c, first.ChunkID, true); err != nil {
		t.Fatalf("ReadChunk failed: %v", err)
	}
	ok, err := VerifyChunk(c, first.ChunkID)
	if err != nil {
		t.Fatalf("VerifyChunk failed: %v", err)
	}
	if !ok {
		t.Fatalf("VerifyChunk returned false")
	}

	_, relOffset, err := MapOffsetToChunk(c, first.SourceOffset)
	if err != nil {
		t.Fatalf("MapOffsetToChunk failed: %v", err)
	}
	if relOffset != 0 {
		t.Fatalf("expected rel offset 0, got %d", relOffset)
	}

	if _, err := ReadFileIndex(c, ""); err != nil {
		t.Fatalf("ReadFileIndex failed: %v", err)
	}

	if _, err := WriteAnalysisResult(c, "analysis/go_sdk_smoke.jsonl", []map[string]any{{"k": "v"}}); err != nil {
		t.Fatalf("WriteAnalysisResult failed: %v", err)
	}

	evt, err := AppendProvenanceEvent(c, "go_sdk_smoke", "test", map[string]any{"ok": true}, "offf-sdk-go-test", "0.1.0")
	if err != nil {
		t.Fatalf("AppendProvenanceEvent failed: %v", err)
	}
	if evt.EventID == "" {
		t.Fatalf("empty event id")
	}
}
