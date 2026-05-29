use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use chrono::Utc;
use offf_access_service::grpc::offf_access_service_client::OfffAccessServiceClient;
use offf_access_service::grpc::{
    AnalysisRow, AppendProvenanceEventRequest, GetChunkRequest, GetFileRequest, GetManifestRequest,
    ListArtifactsRequest, ListFilesRequest, VerifyChunkRequest, WriteAnalysisResultsRequest,
};
use offf_core::{
    chunk::write_chunk,
    parquet_io::{write_file_index, write_physical_to_chunk},
    types::{
        ChunkingInfo, Compression, FileIndexRow, ManifestHashes, ManifestIndexes, ManifestJson,
        SourceInfo, ToolInfo,
    },
};
use serde_json::json;
use tempfile::tempdir;

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn grpc_smoke_all_methods() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    let fixture = build_case_fixture(&root).expect("build fixture");
    let registry_path = root.join("tool-registry.json");
    fs::write(
        &registry_path,
        r#"{
    "tools": [
        {
            "tool_id": "grpc-smoke-test",
            "status": "approved",
            "allowed_roles": ["analysis_worker"],
            "write_layers": ["analysis", "provenance"]
        }
    ]
}"#,
    )
    .expect("write registry fixture");
    let rest_port = pick_free_port().expect("rest port");
    let grpc_port = pick_free_port().expect("grpc port");

    let bin = env!("CARGO_BIN_EXE_offf-access-service");
    let mut child = Command::new(bin)
        .env("OFFF_CASES_ROOT", &root)
        .env("OFFF_ACCESS_BIND", format!("127.0.0.1:{rest_port}"))
        .env("OFFF_ACCESS_GRPC_BIND", format!("127.0.0.1:{grpc_port}"))
        .env("OFFF_TOOL_REGISTRY", &registry_path)
        .env("OFFF_AUTH_MODE", "dev_headers")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn access service");

    let endpoint = format!("http://127.0.0.1:{grpc_port}");
    let mut client = {
        let mut connected = None;
        for _ in 0..40 {
            if let Some(status) = child.try_wait().expect("try_wait") {
                let mut stderr_text = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut stderr_text);
                }
                panic!("access service exited early with status {status}; stderr:\n{stderr_text}");
            }

            match OfffAccessServiceClient::connect(endpoint.clone()).await {
                Ok(client) => {
                    connected = Some(client);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        }

        connected.expect("gRPC server did not become ready")
    };

    fn with_auth<T>(mut req: tonic::Request<T>) -> tonic::Request<T> {
        req.metadata_mut().insert(
            "x-offf-role",
            tonic::metadata::MetadataValue::from_static("analysis_worker"),
        );
        req.metadata_mut().insert(
            "x-offf-tool-id",
            tonic::metadata::MetadataValue::from_static("grpc-smoke-test"),
        );
        req
    }

    let _guard = ChildGuard { child };

    let manifest = client
        .get_manifest(with_auth(tonic::Request::new(GetManifestRequest {
            case_id: fixture.case_id.clone(),
        })))
        .await
        .expect("GetManifest")
        .into_inner();
    assert!(manifest.manifest_json.contains("offf_version"));

    let verify = client
        .verify_chunk(with_auth(tonic::Request::new(VerifyChunkRequest {
            case_id: fixture.case_id.clone(),
            chunk_id: fixture.chunk_id.clone(),
        })))
        .await
        .expect("VerifyChunk")
        .into_inner();
    assert!(verify.ok);

    let chunk = client
        .get_chunk(with_auth(tonic::Request::new(GetChunkRequest {
            case_id: fixture.case_id.clone(),
            chunk_id: fixture.chunk_id.clone(),
        })))
        .await
        .expect("GetChunk")
        .into_inner();
    assert_eq!(chunk.plaintext, b"grpc smoke chunk".to_vec());

    let files = client
        .list_files(with_auth(tonic::Request::new(ListFilesRequest {
            case_id: fixture.case_id.clone(),
            partition_id: "".to_string(),
        })))
        .await
        .expect("ListFiles")
        .into_inner();
    assert!(!files.files.is_empty());

    let file = client
        .get_file(with_auth(tonic::Request::new(GetFileRequest {
            case_id: fixture.case_id.clone(),
            file_id: fixture.file_id,
        })))
        .await
        .expect("GetFile")
        .into_inner();
    let file_row = file.file.expect("GetFile response file");
    assert_eq!(file_row.file_id, fixture.file_id);

    let artifacts_before = client
        .list_artifacts(with_auth(tonic::Request::new(ListArtifactsRequest {
            case_id: fixture.case_id.clone(),
        })))
        .await
        .expect("ListArtifacts before")
        .into_inner();
    assert!(artifacts_before.paths.is_empty());

    let write = client
        .write_analysis_results(with_auth(tonic::Request::new(
            WriteAnalysisResultsRequest {
                case_id: fixture.case_id.clone(),
                relative_path: "analysis/jobs/grpc-smoke-job/grpc_smoke_hits.jsonl".to_string(),
                rows: vec![AnalysisRow {
                    json: json!({"k": "v", "n": 1}).to_string(),
                }],
            },
        )))
        .await
        .expect("WriteAnalysisResults")
        .into_inner();
    assert!(write.ok);

    let artifacts_after = client
        .list_artifacts(with_auth(tonic::Request::new(ListArtifactsRequest {
            case_id: fixture.case_id.clone(),
        })))
        .await
        .expect("ListArtifacts after")
        .into_inner();
    assert!(artifacts_after
        .paths
        .iter()
        .any(|p| p.ends_with("analysis/jobs/grpc-smoke-job/grpc_smoke_hits.jsonl")));

    let write_denied = client
        .write_analysis_results(with_auth(tonic::Request::new(
            WriteAnalysisResultsRequest {
                case_id: fixture.case_id.clone(),
                relative_path: "analysis/jobs/grpc-smoke-job/grpc_smoke_hits.jsonl".to_string(),
                rows: vec![AnalysisRow {
                    json: json!({"k": "v2"}).to_string(),
                }],
            },
        )))
        .await
        .expect_err("second write should be denied");
    assert_eq!(write_denied.code(), tonic::Code::PermissionDenied);

    let denied_log = fs::read_to_string(
        root.join(&fixture.case_id)
            .join("extensions")
            .join("access")
            .join("denied_access_events.jsonl"),
    )
    .expect("denied write log should exist");
    assert!(denied_log.contains("grpc_write_analysis_results"));

    let appended = client
        .append_provenance_event(with_auth(tonic::Request::new(
            AppendProvenanceEventRequest {
                case_id: fixture.case_id,
                action: "grpc_smoke".to_string(),
                actor: "test-runner".to_string(),
                details_json: json!({"scope": "smoke"}).to_string(),
                tool_name: "grpc-test".to_string(),
                tool_version: "0.1.0".to_string(),
            },
        )))
        .await
        .expect("AppendProvenanceEvent")
        .into_inner();
    assert!(appended.ok);
    assert!(appended.event_id.starts_with("evt-"));
}

struct Fixture {
    case_id: String,
    chunk_id: String,
    file_id: u64,
}

fn build_case_fixture(root: &Path) -> Result<Fixture, Box<dyn std::error::Error>> {
    let case_id = "grpc-smoke.offf".to_string();
    let case_path = root.join(&case_id);
    fs::create_dir_all(&case_path)?;

    let chunk = write_chunk(&case_path, 0, 0, b"grpc smoke chunk", &Compression::None)?;

    let map_path = case_path.join("maps").join("physical_to_chunk.parquet");
    write_physical_to_chunk(&map_path, std::slice::from_ref(&chunk))?;

    let manifest = ManifestJson {
        offf_version: "0.1.0".to_string(),
        container_id: "urn:offf:case:grpc-smoke".to_string(),
        created_at: Utc::now(),
        created_by_tool: ToolInfo {
            name: "grpc-smoke-test".to_string(),
            version: "0.1.0".to_string(),
        },
        source: Some(SourceInfo {
            source_type: "raw_image".to_string(),
            size_bytes: chunk.source_length,
            sector_size: 512,
        }),
        hashes: Some(ManifestHashes {
            source_sha256: chunk.plaintext_sha256.clone(),
            merkle_root_sha256: chunk.plaintext_sha256.clone(),
        }),
        chunking: Some(ChunkingInfo {
            chunk_size: chunk.source_length,
            chunking_mode: "fixed".to_string(),
            compression: "none".to_string(),
            hash_algorithm: "sha256".to_string(),
        }),
        indexes: ManifestIndexes {
            physical_to_chunk: Some("maps/physical_to_chunk.parquet".to_string()),
            object_index: None,
            object_edges: None,
        },
        acquisition_mode: None,
        evidence_roots: None,
        limitations: None,
        extensions: None,
    };

    fs::write(
        case_path.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    let file_index_path = case_path
        .join("indexes")
        .join("filesystems")
        .join("volume-1")
        .join("file_index.parquet");
    let file_row = FileIndexRow {
        file_id: 1,
        filesystem_id: "ntfs-1".to_string(),
        partition_id: "volume-1".to_string(),
        path: "/docs/a.txt".to_string(),
        filename: "a.txt".to_string(),
        extension: "txt".to_string(),
        size_bytes: chunk.source_length,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        physical_extents: "[{\"offset\":0,\"length\":15}]".to_string(),
        chunk_refs: format!("[\"{}\"]", chunk.chunk_id),
        is_directory: false,
        is_deleted: false,
        is_sparse: false,
        is_compressed: false,
        is_encrypted: false,
        ads_streams: "[]".to_string(),
        parser: "grpc-smoke".to_string(),
        parser_version: "0.1.0".to_string(),
        parser_status: "ok".to_string(),
        parser_error: "".to_string(),
    };
    write_file_index(&file_index_path, std::slice::from_ref(&file_row))?;

    fs::create_dir_all(case_path.join("analysis"))?;
    fs::create_dir_all(case_path.join("provenance"))?;
    fs::write(
        case_path.join("provenance").join("chain_of_custody.jsonl"),
        "",
    )?;

    Ok(Fixture {
        case_id,
        chunk_id: chunk.chunk_id,
        file_id: file_row.file_id,
    })
}

fn pick_free_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
