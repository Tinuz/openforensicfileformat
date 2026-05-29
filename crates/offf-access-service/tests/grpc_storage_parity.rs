use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use chrono::Utc;
use offf_access_service::grpc::offf_access_service_client::OfffAccessServiceClient;
use offf_access_service::grpc::{
    AnalysisRow, AppendProvenanceEventRequest, GetManifestRequest, ListArtifactsRequest,
    VerifyChunkRequest, WriteAnalysisResultsRequest,
};
use offf_core::{
    chunk::write_chunk,
    parquet_io::{write_file_index, write_physical_to_chunk},
    storage::ContainerRef,
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

#[derive(Clone)]
struct Fixture {
    case_id: String,
    chunk_id: String,
}

#[derive(Clone)]
struct FlowResult {
    manifest_has_version: bool,
    verify_ok: bool,
    artifact_written: bool,
    provenance_appended: bool,
}

#[tokio::test]
async fn grpc_local_vs_s3_case_path_parity() {
    match std::env::var("OFFF_S3_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => {}
        _ => {
            eprintln!("Skipping parity test: OFFF_S3_ENDPOINT is not set");
            return;
        }
    }
    let s3_bucket = match std::env::var("OFFF_S3_TEST_BUCKET") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("Skipping parity test: OFFF_S3_TEST_BUCKET is not set");
            return;
        }
    };

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

    let local_result = run_flow(
        root.to_string_lossy().to_string(),
        fixture.case_id.clone(),
        fixture.chunk_id.clone(),
        registry_path.clone(),
    )
    .await;

    let run_id = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros() * 1000);
    let s3_root = format!("s3://{s3_bucket}/access-parity-{run_id}");
    let s3_case_uri = format!("{s3_root}/{}", fixture.case_id);
    upload_dir_to_container(
        &root.join(&fixture.case_id),
        &ContainerRef::parse(&s3_case_uri).expect("parse s3 case uri"),
    )
    .expect("upload fixture to s3");

    let s3_case_id_without_ext = fixture.case_id.trim_end_matches(".offf").to_string();
    let s3_result = run_flow(
        s3_root,
        s3_case_id_without_ext,
        fixture.chunk_id,
        registry_path,
    )
    .await;

    assert_eq!(
        local_result.manifest_has_version,
        s3_result.manifest_has_version
    );
    assert_eq!(local_result.verify_ok, s3_result.verify_ok);
    assert_eq!(local_result.artifact_written, s3_result.artifact_written);
    assert_eq!(
        local_result.provenance_appended,
        s3_result.provenance_appended
    );
}

async fn run_flow(
    cases_root: String,
    case_id: String,
    chunk_id: String,
    registry_path: PathBuf,
) -> FlowResult {
    let rest_port = pick_free_port().expect("rest port");
    let grpc_port = pick_free_port().expect("grpc port");

    let bin = env!("CARGO_BIN_EXE_offf-access-service");
    let mut cmd = Command::new(bin);
    cmd.env("OFFF_CASES_ROOT", &cases_root)
        .env("OFFF_ACCESS_BIND", format!("127.0.0.1:{rest_port}"))
        .env("OFFF_ACCESS_GRPC_BIND", format!("127.0.0.1:{grpc_port}"))
        .env("OFFF_TOOL_REGISTRY", &registry_path)
        .env("OFFF_AUTH_MODE", "dev_headers")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    for key in [
        "OFFF_S3_ENDPOINT",
        "AWS_REGION",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

    let mut child = cmd.spawn().expect("spawn access service");
    let endpoint = format!("http://127.0.0.1:{grpc_port}");
    let mut client = {
        let mut connected = None;
        for _ in 0..50 {
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
            case_id: case_id.clone(),
        })))
        .await
        .expect("GetManifest")
        .into_inner();

    let verify = client
        .verify_chunk(with_auth(tonic::Request::new(VerifyChunkRequest {
            case_id: case_id.clone(),
            chunk_id,
        })))
        .await
        .expect("VerifyChunk")
        .into_inner();

    let write = client
        .write_analysis_results(with_auth(tonic::Request::new(
            WriteAnalysisResultsRequest {
                case_id: case_id.clone(),
                relative_path: "analysis/jobs/parity-job/parity_hits.jsonl".to_string(),
                rows: vec![AnalysisRow {
                    json: json!({"source": "parity"}).to_string(),
                }],
            },
        )))
        .await
        .expect("WriteAnalysisResults")
        .into_inner();

    let artifacts = client
        .list_artifacts(with_auth(tonic::Request::new(ListArtifactsRequest {
            case_id: case_id.clone(),
        })))
        .await
        .expect("ListArtifacts")
        .into_inner();

    let prov = client
        .append_provenance_event(with_auth(tonic::Request::new(
            AppendProvenanceEventRequest {
                case_id,
                action: "parity_test".to_string(),
                actor: "test-runner".to_string(),
                details_json: json!({"scope": "storage_parity"}).to_string(),
                tool_name: "grpc-test".to_string(),
                tool_version: "0.1.0".to_string(),
            },
        )))
        .await
        .expect("AppendProvenanceEvent")
        .into_inner();

    FlowResult {
        manifest_has_version: manifest.manifest_json.contains("offf_version"),
        verify_ok: verify.ok,
        artifact_written: write.ok
            && artifacts
                .paths
                .iter()
                .any(|p| p.ends_with("analysis/jobs/parity-job/parity_hits.jsonl")),
        provenance_appended: prov.ok && prov.event_id.starts_with("evt-"),
    }
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
    })
}

fn upload_dir_to_container(
    src_root: &Path,
    dst: &ContainerRef,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in walk(src_root)? {
        let rel = entry
            .strip_prefix(src_root)?
            .to_string_lossy()
            .replace('\\', "/");
        let data = fs::read(&entry)?;
        dst.write_bytes(&rel, &data)?;
    }
    Ok(())
}

fn walk(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn pick_free_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
