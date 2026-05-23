package offfsdk

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/klauspost/compress/zstd"
	"github.com/parquet-go/parquet-go"
)

var (
	ErrContainerNotFound = errors.New("container path does not exist")
)

type ChunkRecord struct {
	Sequence       uint64 `parquet:"sequence"`
	ChunkID        string `parquet:"chunk_id"`
	SourceOffset   uint64 `parquet:"source_offset"`
	SourceLength   uint64 `parquet:"source_length"`
	StoredLength   uint64 `parquet:"stored_length"`
	Compression    string `parquet:"compression"`
	PlaintextSHA256 string `parquet:"plaintext_sha256"`
	StoredSHA256   string `parquet:"stored_sha256"`
}

type FileIndexRow struct {
	FileID       uint64 `parquet:"file_id"`
	FilesystemID string `parquet:"filesystem_id"`
	PartitionID  string `parquet:"partition_id"`
	Path         string `parquet:"path"`
	Filename     string `parquet:"filename"`
	Extension    string `parquet:"extension"`
	SizeBytes    uint64 `parquet:"size_bytes"`
	IsDirectory  bool   `parquet:"is_directory"`
	IsDeleted    bool   `parquet:"is_deleted"`
}

type ToolInfo struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

type ProvenanceEvent struct {
	EventID   string                 `json:"event_id"`
	Timestamp string                 `json:"timestamp"`
	Actor     string                 `json:"actor"`
	Action    string                 `json:"action"`
	Tool      ToolInfo               `json:"tool"`
	Details   map[string]any         `json:"details"`
}

type Container struct {
	BasePath string
	manifest map[string]any
	chunks   []ChunkRecord
}

func OpenContainer(containerPath string) (*Container, error) {
	st, err := os.Stat(containerPath)
	if err != nil || !st.IsDir() {
		return nil, fmt.Errorf("%w: %s", ErrContainerNotFound, containerPath)
	}
	manifestPath := filepath.Join(containerPath, "manifest.json")
	manifestRaw, err := os.ReadFile(manifestPath)
	if err != nil {
		return nil, fmt.Errorf("read manifest: %w", err)
	}
	var manifest map[string]any
	if err := json.Unmarshal(manifestRaw, &manifest); err != nil {
		return nil, fmt.Errorf("parse manifest: %w", err)
	}
	chunks, err := loadChunkMap(containerPath, manifest)
	if err != nil {
		return nil, err
	}
	return &Container{BasePath: containerPath, manifest: manifest, chunks: chunks}, nil
}

func ReadManifest(container *Container) (map[string]any, error) {
	if container == nil {
		return nil, errors.New("container is nil")
	}
	out := make(map[string]any, len(container.manifest))
	for k, v := range container.manifest {
		out[k] = v
	}
	return out, nil
}

func VerifyContainer(container *Container) (map[string]bool, error) {
	if container == nil {
		return nil, errors.New("container is nil")
	}
	sourceOK, err := verifySourceHash(container)
	if err != nil {
		return nil, err
	}
	merkleOK, err := verifyMerkleRootBin(container)
	if err != nil {
		return nil, err
	}
	result := map[string]bool{
		"schema":      true,
		"source_hash": sourceOK,
		"merkle_root": merkleOK,
		"valid":       sourceOK && merkleOK,
	}
	return result, nil
}

func ReadChunk(container *Container, chunkID string, verify bool) ([]byte, error) {
	chunk, err := findChunkByID(container, chunkID)
	if err != nil {
		return nil, err
	}
	return readChunkPlaintext(container.BasePath, chunk, verify)
}

func VerifyChunk(container *Container, chunkID string) (bool, error) {
	_, err := ReadChunk(container, chunkID, true)
	if err != nil {
		return false, err
	}
	return true, nil
}

func MapOffsetToChunk(container *Container, sourceOffset uint64) (ChunkRecord, uint64, error) {
	if container == nil {
		return ChunkRecord{}, 0, errors.New("container is nil")
	}
	for _, chunk := range container.chunks {
		start := chunk.SourceOffset
		end := chunk.SourceOffset + chunk.SourceLength
		if sourceOffset >= start && sourceOffset < end {
			return chunk, sourceOffset - start, nil
		}
	}
	return ChunkRecord{}, 0, fmt.Errorf("source offset outside chunk map: %d", sourceOffset)
}

func ReadFileIndex(container *Container, partitionID string) ([]map[string]any, error) {
	if container == nil {
		return nil, errors.New("container is nil")
	}
	fsRoot := filepath.Join(container.BasePath, "indexes", "filesystems")
	entries, err := os.ReadDir(fsRoot)
	if err != nil {
		return nil, fmt.Errorf("read filesystems index root: %w", err)
	}

	var paths []string
	if partitionID != "" {
		paths = append(paths, filepath.Join(fsRoot, partitionID, "file_index.parquet"))
	} else {
		for _, e := range entries {
			if e.IsDir() {
				paths = append(paths, filepath.Join(fsRoot, e.Name(), "file_index.parquet"))
			}
		}
	}

	var out []map[string]any
	for _, p := range paths {
		if _, err := os.Stat(p); err != nil {
			continue
		}
		rows, err := readParquetAsMaps(p)
		if err != nil {
			return nil, err
		}
		out = append(out, rows...)
	}
	return out, nil
}

func WriteAnalysisResult(container *Container, relativePath string, rows []map[string]any) (string, error) {
	if container == nil {
		return "", errors.New("container is nil")
	}
	rel := strings.ReplaceAll(relativePath, "\\", "/")
	rel = strings.TrimPrefix(rel, "/")
	if !strings.HasPrefix(rel, "analysis/") {
		return "", errors.New("relative path must start with analysis/")
	}
	if !strings.HasSuffix(rel, ".jsonl") {
		return "", errors.New("only .jsonl is supported")
	}
	target := filepath.Join(container.BasePath, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return "", err
	}
	f, err := os.OpenFile(target, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return "", err
	}
	defer f.Close()
	w := bufio.NewWriter(f)
	for _, row := range rows {
		line, err := json.Marshal(row)
		if err != nil {
			return "", err
		}
		if _, err := w.Write(line); err != nil {
			return "", err
		}
		if _, err := w.WriteString("\n"); err != nil {
			return "", err
		}
	}
	if err := w.Flush(); err != nil {
		return "", err
	}
	return rel, nil
}

func AppendProvenanceEvent(
	container *Container,
	action string,
	actor string,
	details map[string]any,
	toolName string,
	toolVersion string,
) (ProvenanceEvent, error) {
	if container == nil {
		return ProvenanceEvent{}, errors.New("container is nil")
	}
	if toolName == "" {
		toolName = "offf-sdk-go"
	}
	if toolVersion == "" {
		toolVersion = "0.1.0"
	}
	path := filepath.Join(container.BasePath, "provenance", "chain_of_custody.jsonl")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return ProvenanceEvent{}, err
	}
	counter := 0
	if data, err := os.ReadFile(path); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			if strings.TrimSpace(line) != "" {
				counter++
			}
		}
	}

	evt := ProvenanceEvent{
		EventID:   fmt.Sprintf("evt-%06d", counter),
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Actor:     actor,
		Action:    action,
		Tool: ToolInfo{
			Name:    toolName,
			Version: toolVersion,
		},
		Details: details,
	}

	line, err := json.Marshal(evt)
	if err != nil {
		return ProvenanceEvent{}, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return ProvenanceEvent{}, err
	}
	defer f.Close()
	if _, err := f.Write(append(line, '\n')); err != nil {
		return ProvenanceEvent{}, err
	}
	return evt, nil
}

func loadChunkMap(basePath string, manifest map[string]any) ([]ChunkRecord, error) {
	indexes, ok := manifest["indexes"].(map[string]any)
	if !ok {
		return nil, errors.New("manifest.indexes missing or invalid")
	}
	mapRel, ok := indexes["physical_to_chunk"].(string)
	if !ok || mapRel == "" {
		return nil, errors.New("manifest.indexes.physical_to_chunk missing")
	}
	mapPath := filepath.Join(basePath, filepath.FromSlash(mapRel))
	rows, err := parquet.ReadFile[ChunkRecord](mapPath)
	if err != nil {
		return nil, fmt.Errorf("read chunk map parquet: %w", err)
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].Sequence < rows[j].Sequence })
	return rows, nil
}

func readChunkPlaintext(basePath string, chunk ChunkRecord, verify bool) ([]byte, error) {
	hexHash := strings.TrimPrefix(chunk.PlaintextSHA256, "sha256:")
	path := filepath.Join(basePath, "chunks", "sha256", hexHash[0:2], hexHash[2:4], hexHash+".chunk")
	stored, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if verify {
		storedHash := sha256.Sum256(stored)
		if hex.EncodeToString(storedHash[:]) != chunk.StoredSHA256 {
			return nil, fmt.Errorf("stored hash mismatch for chunk %s", chunk.ChunkID)
		}
	}

	var plain []byte
	switch chunk.Compression {
	case "none":
		plain = stored
	case "zstd":
		zr, err := zstd.NewReader(nil)
		if err != nil {
			return nil, err
		}
		defer zr.Close()
		plain, err = zr.DecodeAll(stored, nil)
		if err != nil {
			return nil, err
		}
	default:
		return nil, fmt.Errorf("unsupported compression: %s", chunk.Compression)
	}

	if verify {
		plainHash := sha256.Sum256(plain)
		if hex.EncodeToString(plainHash[:]) != chunk.PlaintextSHA256 {
			return nil, fmt.Errorf("plaintext hash mismatch for chunk %s", chunk.ChunkID)
		}
	}
	return plain, nil
}

func verifySourceHash(container *Container) (bool, error) {
	h := sha256.New()
	for _, chunk := range container.chunks {
		plain, err := readChunkPlaintext(container.BasePath, chunk, true)
		if err != nil {
			return false, err
		}
		if _, err := h.Write(plain); err != nil {
			return false, err
		}
	}
	hashes, ok := container.manifest["hashes"].(map[string]any)
	if !ok {
		return false, errors.New("manifest.hashes missing")
	}
	expected, _ := hashes["source_sha256"].(string)
	return hex.EncodeToString(h.Sum(nil)) == expected, nil
}

func verifyMerkleRootBin(container *Container) (bool, error) {
	hashes, ok := container.manifest["hashes"].(map[string]any)
	if !ok {
		return false, errors.New("manifest.hashes missing")
	}
	expected, _ := hashes["merkle_root_sha256"].(string)
	if expected == "" {
		return false, errors.New("manifest.hashes.merkle_root_sha256 missing")
	}
	data, err := os.ReadFile(filepath.Join(container.BasePath, "hashes", "merkle_tree.bin"))
	if err != nil {
		return false, err
	}
	if len(data) < 32 {
		return false, errors.New("merkle_tree.bin too short")
	}
	actual := hex.EncodeToString(data[len(data)-32:])
	return actual == expected, nil
}

func findChunkByID(container *Container, chunkID string) (ChunkRecord, error) {
	if container == nil {
		return ChunkRecord{}, errors.New("container is nil")
	}
	for _, chunk := range container.chunks {
		if chunk.ChunkID == chunkID {
			return chunk, nil
		}
	}
	return ChunkRecord{}, fmt.Errorf("chunk not found: %s", chunkID)
}

func readParquetAsMaps(path string) ([]map[string]any, error) {
	rows, err := parquet.ReadFile[FileIndexRow](path)
	if err != nil {
		return nil, err
	}

	out := make([]map[string]any, 0, len(rows))
	for _, row := range rows {
		out = append(out, map[string]any{
			"file_id":       row.FileID,
			"filesystem_id": row.FilesystemID,
			"partition_id":  row.PartitionID,
			"path":          row.Path,
			"filename":      row.Filename,
			"extension":     row.Extension,
			"size_bytes":    row.SizeBytes,
			"is_directory":  row.IsDirectory,
			"is_deleted":    row.IsDeleted,
		})
	}
	return out, nil
}
