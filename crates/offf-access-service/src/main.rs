use std::{env, fs, net::SocketAddr, path::Path};

use anyhow::{Context, Result};
use arrow::{
    array::{BooleanArray, StringArray, UInt64Array},
    record_batch::RecordBatch,
};
use axum::{
    extract::{DefaultBodyLimit, Path as AxPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
    Json, Router,
};
use base64ct::{Base64UrlUnpadded, Encoding};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use chrono::Utc;
use offf_core::{
    chunk::hex_sha256,
    parquet_io::read_physical_to_chunk_bytes,
    storage::{
        read_chunk_verified, read_derived_object, read_file_verified, read_object_verified,
        write_derived_object, ContainerRef,
    },
    types::{ChunkMetadata, ManifestJson, ToolInfo},
};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tonic::{Code, Request, Response as GrpcResponse, Status};
use tower_http::trace::TraceLayer;

use grpc::offf_access_service_server::{OfffAccessService, OfffAccessServiceServer};
use grpc::{
    AppendAnalysisCorrectionRequest, AppendAnalysisCorrectionResponse,
    AppendProvenanceEventRequest, AppendProvenanceEventResponse, FileRow, GetChunkRequest,
    GetChunkResponse, GetFileRequest, GetFileResponse, GetManifestRequest, GetManifestResponse,
    GetObjectChildrenRequest, GetObjectChildrenResponse, GetObjectContentRequest,
    GetObjectContentResponse, GetObjectLineageRequest, GetObjectLineageResponse,
    GetObjectParentsRequest, GetObjectParentsResponse, GetObjectsRequest, GetObjectsResponse,
    ListArtifactsRequest, ListArtifactsResponse, ListFilesRequest, ListFilesResponse,
    VerifyChunkRequest, VerifyChunkResponse as GrpcVerifyChunkResponse,
    WriteAnalysisResultsRequest, WriteAnalysisResultsResponse, WriteDerivationDeltaRequest,
    WriteDerivationDeltaResponse, WriteMaterializedObjectRequest, WriteMaterializedObjectResponse,
    WriteObjectDeltaRequest, WriteObjectDeltaResponse, WriteObjectEdgeDeltaRequest,
    WriteObjectEdgeDeltaResponse,
};
use offf_access_service::grpc;

/// T-10: Maximum number of rows accepted in a single write request (DoS guard).
const MAX_ROWS_PER_REQUEST: usize = 50_000;

#[derive(Clone)]
struct AppState {
    cases_root: String,
    tool_registry_path: String,
    auth_mode: AuthMode,
    /// HMAC-SHA256 signing key for JWT mode. Required when auth_mode == Jwt.
    jwt_secret: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug)]
enum AuthMode {
    DevHeaders,
    Jwt,
    Mtls,
}

impl AuthMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev_headers" | "dev-headers" => Some(Self::DevHeaders),
            "jwt" => Some(Self::Jwt),
            "mtls" => Some(Self::Mtls),
            _ => None,
        }
    }

    fn role_header(self) -> &'static str {
        match self {
            Self::DevHeaders => "x-offf-role",
            Self::Jwt => "x-offf-claim-role",
            Self::Mtls => "x-offf-cert-role",
        }
    }

    fn tool_header(self) -> &'static str {
        match self {
            Self::DevHeaders => "x-offf-tool-id",
            Self::Jwt => "x-offf-claim-tool-id",
            Self::Mtls => "x-offf-cert-tool-id",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::DevHeaders => "dev_headers",
            Self::Jwt => "jwt",
            Self::Mtls => "mtls",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppRole {
    Viewer,
    Reporter,
    AnalysisWorker,
    Indexer,
    AcquisitionProducer,
}

impl AppRole {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "viewer" => Some(Self::Viewer),
            "reporter" => Some(Self::Reporter),
            "analysis_worker" | "analysis-worker" => Some(Self::AnalysisWorker),
            "indexer" => Some(Self::Indexer),
            "acquisition_producer" | "acquisition-producer" => Some(Self::AcquisitionProducer),
            _ => None,
        }
    }

    fn can_write_analysis(self) -> bool {
        matches!(self, Self::AnalysisWorker)
    }

    fn can_append_provenance(self) -> bool {
        matches!(
            self,
            Self::AnalysisWorker | Self::Indexer | Self::AcquisitionProducer
        )
    }

    fn can_append_analysis_correction(self) -> bool {
        matches!(self, Self::AnalysisWorker | Self::Reporter)
    }
}

#[derive(Clone, Debug)]
struct ActorContext {
    role: AppRole,
    tool_id: String,
}

#[derive(Debug, Deserialize)]
struct ToolRegistryFile {
    tools: Vec<ToolRegistration>,
}

#[derive(Debug, Deserialize)]
struct ToolRegistration {
    tool_id: String,
    status: String,
    allowed_roles: Vec<String>,
    write_layers: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug)]
enum AuthError {
    Unauthorized(String),
}

const DENIED_WRITES_REL: &str = "extensions/access/denied_access_events.jsonl";
const ACCESS_EVENTS_REL: &str = "extensions/access/access_events.jsonl";
const ACCESS_DENIED_REL: &str = "extensions/access/denied_access_events.jsonl";
const ACCESS_TOOL_NAME: &str = "offf-access-service";
const ACCESS_TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy)]
enum WriteLayer {
    Analysis,
    Provenance,
}

impl WriteLayer {
    fn as_str(self) -> &'static str {
        match self {
            WriteLayer::Analysis => "analysis",
            WriteLayer::Provenance => "provenance",
        }
    }
}

#[derive(Debug, Deserialize)]
struct FilesQuery {
    partition_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalysisResultsRequest {
    relative_path: String,
    rows: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct ProvenanceEventRequest {
    action: String,
    actor: String,
    details: Value,
    tool_name: Option<String>,
    tool_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalysisCorrectionRequest {
    actor: String,
    correction_of: String,
    correction_type: String,
    reason: String,
    provenance_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifyChunkResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct WriteResultResponse {
    ok: bool,
    path: String,
}

#[derive(Debug, Serialize)]
struct AppendProvenanceResponse {
    ok: bool,
    event_id: String,
    timestamp: String,
}

#[derive(Debug, Serialize)]
struct AppendAnalysisCorrectionJsonResponse {
    ok: bool,
    event_id: String,
    timestamp: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct ObjectDeltaRequest {
    rows: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct ObjectDeltaResponse {
    ok: bool,
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct WriteMaterializedObjectRestResponse {
    ok: bool,
    sha256: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cases_root = env::var("OFFF_CASES_ROOT").unwrap_or_else(|_| "tests/samples".to_string());
    let rest_bind = env::var("OFFF_ACCESS_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let grpc_bind =
        env::var("OFFF_ACCESS_GRPC_BIND").unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    let tool_registry_path =
        env::var("OFFF_TOOL_REGISTRY").unwrap_or_else(|_| "config/tool-registry.json".to_string());
    let auth_mode = env::var("OFFF_AUTH_MODE")
        .ok()
        .as_deref()
        .and_then(AuthMode::parse)
        .unwrap_or(AuthMode::DevHeaders);
    let jwt_secret: Option<Vec<u8>> = env::var("OFFF_JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.into_bytes());
    if matches!(auth_mode, AuthMode::Jwt) && jwt_secret.is_none() {
        anyhow::bail!("OFFF_JWT_SECRET must be set when OFFF_AUTH_MODE=jwt");
    }
    let rest_addr: SocketAddr = rest_bind.parse().context("invalid OFFF_ACCESS_BIND")?;
    let grpc_addr: SocketAddr = grpc_bind.parse().context("invalid OFFF_ACCESS_GRPC_BIND")?;

    let state = AppState {
        cases_root,
        tool_registry_path,
        auth_mode,
        jwt_secret,
    };

    tracing::info!(auth_mode = %state.auth_mode.as_str(), "access auth mode configured");

    let app = Router::new()
        .route("/cases/{case_id}/manifest", get(get_manifest))
        .route("/cases/{case_id}/chunks/{chunk_id}", get(get_chunk))
        .route(
            "/cases/{case_id}/chunks/{chunk_id}/verify",
            get(verify_chunk_endpoint),
        )
        .route("/cases/{case_id}/files", get(list_files))
        .route("/cases/{case_id}/files/{file_id}", get(get_file))
        .route("/cases/{case_id}/artifacts", get(list_artifacts))
        .route(
            "/cases/{case_id}/analysis-results",
            post(write_analysis_results),
        )
        .route(
            "/cases/{case_id}/analysis-corrections",
            post(append_analysis_correction),
        )
        .route(
            "/cases/{case_id}/provenance-events",
            post(append_provenance_event),
        )
        // Object-producing worker endpoints (Sprint 11)
        .route(
            "/cases/{case_id}/analysis/jobs/{job_id}/objects",
            post(rest_write_object_delta),
        )
        .route(
            "/cases/{case_id}/analysis/jobs/{job_id}/object-edges",
            post(rest_write_object_edge_delta),
        )
        .route(
            "/cases/{case_id}/analysis/jobs/{job_id}/derivations",
            post(rest_write_derivation_delta),
        )
        .route(
            "/cases/{case_id}/analysis/jobs/{job_id}/materialized-objects",
            post(rest_write_materialized_object),
        )
        .route("/cases/{case_id}/objects", get(rest_get_objects))
        .route(
            "/cases/{case_id}/objects/{object_id}/children",
            get(rest_get_object_children),
        )
        .route(
            "/cases/{case_id}/objects/{object_id}/parents",
            get(rest_get_object_parents),
        )
        .route(
            "/cases/{case_id}/objects/{object_id}/lineage",
            get(rest_get_object_lineage),
        )
        .route(
            "/cases/{case_id}/objects/{object_id}/content",
            get(rest_get_object_content),
        )
        .route(
            "/cases/{case_id}/files/{filesystem_id}/{file_id}/content",
            get(rest_get_file_content),
        )
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // T-10: 10 MB request body limit
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let grpc_service = GrpcAccessService { state };

    let listener = TcpListener::bind(rest_addr).await?;
    let rest_server = async move {
        tracing::info!("OFFF Access REST listening on {rest_addr}");
        axum::serve(listener, app)
            .await
            .context("REST server failed")
    };

    let grpc_server = async move {
        tracing::info!("OFFF Access gRPC listening on {grpc_addr}");
        tonic::transport::Server::builder()
            .add_service(OfffAccessServiceServer::new(grpc_service))
            .serve(grpc_addr)
            .await
            .context("gRPC server failed")
    };

    tokio::try_join!(rest_server, grpc_server)?;
    Ok(())
}

#[derive(Clone)]
struct GrpcAccessService {
    state: AppState,
}

#[tonic::async_trait]
impl OfffAccessService for GrpcAccessService {
    async fn get_manifest(
        &self,
        request: Request<GetManifestRequest>,
    ) -> Result<GrpcResponse<GetManifestResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let value = read_json_file(&case_ref, "manifest.json").map_err(grpc_status)?;
        let manifest_json = serde_json::to_string(&value)
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(GrpcResponse::new(GetManifestResponse { manifest_json }))
    }

    async fn get_chunk(
        &self,
        request: Request<GetChunkRequest>,
    ) -> Result<GrpcResponse<GetChunkResponse>, Status> {
        let req = request.into_inner();
        let (case_ref, _, chunks) =
            load_case_data(&self.state, &req.case_id).map_err(grpc_status)?;
        let chunk = find_chunk(&chunks, &req.chunk_id).map_err(grpc_status)?;
        let plaintext = read_chunk_verified(&case_ref, chunk)
            .map_err(ApiError::from)
            .map_err(grpc_status)?;
        Ok(GrpcResponse::new(GetChunkResponse { plaintext }))
    }

    async fn verify_chunk(
        &self,
        request: Request<VerifyChunkRequest>,
    ) -> Result<GrpcResponse<GrpcVerifyChunkResponse>, Status> {
        let req = request.into_inner();
        let (case_ref, _, chunks) =
            load_case_data(&self.state, &req.case_id).map_err(grpc_status)?;
        let chunk = find_chunk(&chunks, &req.chunk_id).map_err(grpc_status)?;
        read_chunk_verified(&case_ref, chunk)
            .map_err(ApiError::from)
            .map_err(grpc_status)?;
        Ok(GrpcResponse::new(GrpcVerifyChunkResponse { ok: true }))
    }

    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<GrpcResponse<ListFilesResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let partition = if req.partition_id.trim().is_empty() {
            None
        } else {
            Some(req.partition_id.as_str())
        };
        let rows = list_file_index_values(&case_ref, partition).map_err(grpc_status)?;
        let files = rows.iter().map(value_to_file_row).collect();
        Ok(GrpcResponse::new(ListFilesResponse { files }))
    }

    async fn get_file(
        &self,
        request: Request<GetFileRequest>,
    ) -> Result<GrpcResponse<GetFileResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows = list_file_index_values(&case_ref, None).map_err(grpc_status)?;
        let file = rows
            .iter()
            .find(|row| row.get("file_id").and_then(|v| v.as_u64()) == Some(req.file_id))
            .map(value_to_file_row)
            .ok_or_else(|| Status::new(Code::NotFound, "file not found"))?;
        Ok(GrpcResponse::new(GetFileResponse { file: Some(file) }))
    }

    async fn list_artifacts(
        &self,
        request: Request<ListArtifactsRequest>,
    ) -> Result<GrpcResponse<ListArtifactsResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let mut out = list_analysis_artifacts(&case_ref).map_err(grpc_status)?;
        out.sort();
        Ok(GrpcResponse::new(ListArtifactsResponse { paths: out }))
    }

    async fn write_analysis_results(
        &self,
        request: Request<WriteAnalysisResultsRequest>,
    ) -> Result<GrpcResponse<WriteAnalysisResultsResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(actor) => actor,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_analysis_results",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_write_analysis() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_analysis_results",
                "role cannot write analysis",
                Some(&actor),
            );
            tracing::warn!(
                action = "grpc_write_analysis_results",
                tool_id = %actor.tool_id,
                role = ?actor.role,
                outcome = "deny",
                reason = "role cannot write analysis"
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to write analysis layer",
            ));
        }
        if let Err(err) = enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis) {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_analysis_results",
                &err.message,
                Some(&actor),
            );
            return Err(grpc_status(err));
        }

        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;

        let mut rows = Vec::with_capacity(req.rows.len());
        for row in req.rows {
            let value: Value = serde_json::from_str(&row.json)
                .map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?;
            rows.push(value);
        }

        let target = match write_analysis_rows(&case_ref, &req.relative_path, &rows) {
            Ok(target) => target,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &req.case_id,
                    "grpc_write_analysis_results",
                    &err.message,
                    Some(&actor),
                );
                return Err(grpc_status(err));
            }
        };
        tracing::info!(
            action = "grpc_write_analysis_results",
            case_id = %req.case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "allow",
            path = %target,
        );
        Ok(GrpcResponse::new(WriteAnalysisResultsResponse {
            ok: true,
            path: target,
        }))
    }

    async fn append_provenance_event(
        &self,
        request: Request<AppendProvenanceEventRequest>,
    ) -> Result<GrpcResponse<AppendProvenanceEventResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(actor) => actor,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_append_provenance_event",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_append_provenance() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_append_provenance_event",
                "role cannot append provenance",
                Some(&actor),
            );
            tracing::warn!(
                action = "grpc_append_provenance_event",
                tool_id = %actor.tool_id,
                role = ?actor.role,
                outcome = "deny",
                reason = "role cannot append provenance"
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to append provenance",
            ));
        }
        if let Err(err) = enforce_tool_registry(&self.state, &actor, WriteLayer::Provenance) {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_append_provenance_event",
                &err.message,
                Some(&actor),
            );
            return Err(grpc_status(err));
        }

        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;

        let details = if req.details_json.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&req.details_json)
                .map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?
        };

        let appended = append_provenance(
            &case_ref,
            &req.action,
            &req.actor,
            details,
            Some(req.tool_name),
            Some(req.tool_version),
        )
        .map_err(grpc_status)?;

        tracing::info!(
            action = "grpc_append_provenance_event",
            case_id = %req.case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "allow",
            event_id = %appended.event_id,
        );

        Ok(GrpcResponse::new(AppendProvenanceEventResponse {
            ok: true,
            event_id: appended.event_id,
            timestamp: appended.timestamp,
        }))
    }

    async fn append_analysis_correction(
        &self,
        request: Request<AppendAnalysisCorrectionRequest>,
    ) -> Result<GrpcResponse<AppendAnalysisCorrectionResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(actor) => actor,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_append_analysis_correction",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_append_analysis_correction() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_append_analysis_correction",
                "role cannot append analysis correction",
                Some(&actor),
            );
            tracing::warn!(
                action = "grpc_append_analysis_correction",
                tool_id = %actor.tool_id,
                role = ?actor.role,
                outcome = "deny",
                reason = "role cannot append analysis correction"
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to append analysis correction",
            ));
        }
        if let Err(err) = enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis) {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_append_analysis_correction",
                &err.message,
                Some(&actor),
            );
            return Err(grpc_status(err));
        }

        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let appended = append_analysis_correction_event(
            &case_ref,
            &req.actor,
            &req.correction_of,
            &req.correction_type,
            &req.reason,
            if req.provenance_ref.trim().is_empty() {
                None
            } else {
                Some(req.provenance_ref)
            },
        )
        .map_err(grpc_status)?;

        tracing::info!(
            action = "grpc_append_analysis_correction",
            case_id = %req.case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "allow",
            event_id = %appended.event_id,
            path = %appended.path,
        );

        Ok(GrpcResponse::new(AppendAnalysisCorrectionResponse {
            ok: true,
            event_id: appended.event_id,
            timestamp: appended.timestamp,
            path: appended.path,
        }))
    }

    // ── Object-producing worker RPCs (Sprint 11) ──────────────────────────

    async fn write_object_delta(
        &self,
        request: Request<WriteObjectDeltaRequest>,
    ) -> Result<GrpcResponse<WriteObjectDeltaResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(a) => a,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_delta",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_write_analysis() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_object_delta",
                "role cannot write analysis",
                Some(&actor),
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to write analysis layer",
            ));
        }
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis)
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        enforce_capability(&self.state, &actor, "may_produce_objects")
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows: Result<Vec<Value>, _> = req
            .rows
            .iter()
            .map(|r| serde_json::from_str::<Value>(&r.json))
            .collect();
        let rows = rows.map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?;
        let (path, sha256) =
            write_object_delta_rows(&case_ref, &req.job_id, "objects_delta", &rows)
                .map_err(grpc_status)?;
        Ok(GrpcResponse::new(WriteObjectDeltaResponse {
            ok: true,
            path,
            sha256,
        }))
    }

    async fn write_object_edge_delta(
        &self,
        request: Request<WriteObjectEdgeDeltaRequest>,
    ) -> Result<GrpcResponse<WriteObjectEdgeDeltaResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(a) => a,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_edge_delta",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_write_analysis() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_object_edge_delta",
                "role cannot write analysis",
                Some(&actor),
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to write analysis layer",
            ));
        }
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis)
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_edge_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        enforce_capability(&self.state, &actor, "may_produce_edges")
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_object_edge_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows: Result<Vec<Value>, _> = req
            .rows
            .iter()
            .map(|r| serde_json::from_str::<Value>(&r.json))
            .collect();
        let rows = rows.map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?;
        let (path, sha256) =
            write_object_delta_rows(&case_ref, &req.job_id, "object_edges_delta", &rows)
                .map_err(grpc_status)?;
        Ok(GrpcResponse::new(WriteObjectEdgeDeltaResponse {
            ok: true,
            path,
            sha256,
        }))
    }

    async fn write_derivation_delta(
        &self,
        request: Request<WriteDerivationDeltaRequest>,
    ) -> Result<GrpcResponse<WriteDerivationDeltaResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(a) => a,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_derivation_delta",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_write_analysis() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_derivation_delta",
                "role cannot write analysis",
                Some(&actor),
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to write analysis layer",
            ));
        }
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis)
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_derivation_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        enforce_capability(&self.state, &actor, "may_produce_derivations")
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_derivation_delta",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows: Result<Vec<Value>, _> = req
            .rows
            .iter()
            .map(|r| serde_json::from_str::<Value>(&r.json))
            .collect();
        let rows = rows.map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?;
        let (path, sha256) =
            write_object_delta_rows(&case_ref, &req.job_id, "derivations_delta", &rows)
                .map_err(grpc_status)?;
        Ok(GrpcResponse::new(WriteDerivationDeltaResponse {
            ok: true,
            path,
            sha256,
        }))
    }

    async fn write_materialized_object(
        &self,
        request: Request<WriteMaterializedObjectRequest>,
    ) -> Result<GrpcResponse<WriteMaterializedObjectResponse>, Status> {
        let actor = match actor_from_metadata(self.state.auth_mode, request.metadata(), self.state.jwt_secret.as_deref()) {
            Ok(a) => a,
            Err(err) => {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_materialized_object",
                    "authentication failed",
                    None,
                );
                return Err(grpc_status_from_auth(err));
            }
        };
        if !actor.role.can_write_analysis() {
            log_denied_write_best_effort(
                &self.state,
                &request.get_ref().case_id,
                "grpc_write_materialized_object",
                "role cannot write analysis",
                Some(&actor),
            );
            return Err(Status::new(
                Code::PermissionDenied,
                "role not allowed to write analysis layer",
            ));
        }
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis)
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_materialized_object",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        enforce_capability(&self.state, &actor, "may_materialize_objects")
            .inspect_err(|err| {
                log_denied_write_best_effort(
                    &self.state,
                    &request.get_ref().case_id,
                    "grpc_write_materialized_object",
                    &err.message,
                    Some(&actor),
                );
            })
            .map_err(grpc_status)?;
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let sha256 = write_derived_object(&case_ref, &req.content)
            .map_err(ApiError::from)
            .map_err(grpc_status)?;
        tracing::info!(
            action = "grpc_write_materialized_object",
            case_id = %req.case_id,
            tool_id = %actor.tool_id,
            sha256 = %sha256,
            outcome = "allow",
        );
        Ok(GrpcResponse::new(WriteMaterializedObjectResponse {
            ok: true,
            sha256,
        }))
    }

    async fn get_objects(
        &self,
        request: Request<GetObjectsRequest>,
    ) -> Result<GrpcResponse<GetObjectsResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows = read_object_index_values(&case_ref).map_err(grpc_status)?;
        let rows_json: Result<Vec<String>, _> = rows.iter().map(serde_json::to_string).collect();
        let rows_json = rows_json.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(GrpcResponse::new(GetObjectsResponse { rows_json }))
    }

    async fn get_object_children(
        &self,
        request: Request<GetObjectChildrenRequest>,
    ) -> Result<GrpcResponse<GetObjectChildrenResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows = read_object_children(&case_ref, &req.object_id).map_err(grpc_status)?;
        let rows_json: Result<Vec<String>, _> = rows.iter().map(serde_json::to_string).collect();
        let rows_json = rows_json.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(GrpcResponse::new(GetObjectChildrenResponse { rows_json }))
    }

    async fn get_object_parents(
        &self,
        request: Request<GetObjectParentsRequest>,
    ) -> Result<GrpcResponse<GetObjectParentsResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows = read_object_parents(&case_ref, &req.object_id).map_err(grpc_status)?;
        let rows_json: Result<Vec<String>, _> = rows.iter().map(serde_json::to_string).collect();
        let rows_json = rows_json.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(GrpcResponse::new(GetObjectParentsResponse { rows_json }))
    }

    async fn get_object_lineage(
        &self,
        request: Request<GetObjectLineageRequest>,
    ) -> Result<GrpcResponse<GetObjectLineageResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let rows = read_object_lineage(&case_ref, &req.object_id).map_err(grpc_status)?;
        let rows_json: Result<Vec<String>, _> = rows.iter().map(serde_json::to_string).collect();
        let rows_json = rows_json.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(GrpcResponse::new(GetObjectLineageResponse { rows_json }))
    }

    async fn get_object_content(
        &self,
        request: Request<GetObjectContentRequest>,
    ) -> Result<GrpcResponse<GetObjectContentResponse>, Status> {
        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;
        let content = read_derived_object(&case_ref, &req.sha256)
            .map_err(ApiError::from)
            .map_err(grpc_status)?;
        Ok(GrpcResponse::new(GetObjectContentResponse { content }))
    }
}

async fn get_manifest(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
) -> Result<Json<Value>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let value = read_json_file(&case_ref, "manifest.json")?;
    Ok(Json(value))
}

async fn get_chunk(
    State(state): State<AppState>,
    AxPath((case_id, chunk_id)): AxPath<(String, String)>,
) -> Result<AxumResponse, ApiError> {
    let (case_ref, manifest, chunks) = load_case_data(&state, &case_id)?;
    let _ = manifest;
    let chunk = find_chunk(&chunks, &chunk_id)?;
    let body = read_chunk_verified(&case_ref, chunk).map_err(ApiError::from)?;

    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response())
}

async fn verify_chunk_endpoint(
    State(state): State<AppState>,
    AxPath((case_id, chunk_id)): AxPath<(String, String)>,
) -> Result<Json<VerifyChunkResponse>, ApiError> {
    let (case_ref, _, chunks) = load_case_data(&state, &case_id)?;
    let chunk = find_chunk(&chunks, &chunk_id)?;
    read_chunk_verified(&case_ref, chunk).map_err(ApiError::from)?;
    Ok(Json(VerifyChunkResponse { ok: true }))
}

async fn list_files(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let rows = list_file_index_values(&case_ref, query.partition_id.as_deref())?;

    Ok(Json(rows))
}

async fn get_file(
    State(state): State<AppState>,
    AxPath((case_id, file_id)): AxPath<(String, u64)>,
) -> Result<Json<Value>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let rows = list_file_index_values(&case_ref, None)?;
    for row in rows {
        if row.get("file_id").and_then(|v| v.as_u64()) == Some(file_id) {
            return Ok(Json(row));
        }
    }

    Err(ApiError::not_found("file not found"))
}

async fn list_artifacts(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let mut out = list_analysis_artifacts(&case_ref)?;
    out.sort();
    Ok(Json(out))
}

async fn write_analysis_results(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
    headers: HeaderMap,
    Json(payload): Json<AnalysisResultsRequest>,
) -> Result<Json<WriteResultResponse>, ApiError> {
    let actor = match actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()) {
        Ok(actor) => actor,
        Err(err) => {
            log_denied_write_best_effort(
                &state,
                &case_id,
                "write_analysis_results",
                &err.message,
                None,
            );
            return Err(err);
        }
    };
    if !actor.role.can_write_analysis() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_analysis_results",
            "role cannot write analysis",
            Some(&actor),
        );
        tracing::warn!(
            action = "write_analysis_results",
            case_id = %case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "deny",
            reason = "role cannot write analysis"
        );
        return Err(ApiError::forbidden(
            "role not allowed to write analysis layer",
        ));
    }
    if let Err(err) = enforce_tool_registry(&state, &actor, WriteLayer::Analysis) {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_analysis_results",
            &err.message,
            Some(&actor),
        );
        return Err(err);
    }

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let target = match write_analysis_rows(&case_ref, &payload.relative_path, &payload.rows) {
        Ok(target) => target,
        Err(err) => {
            log_denied_write_best_effort(
                &state,
                &case_id,
                "write_analysis_results",
                &err.message,
                Some(&actor),
            );
            return Err(err);
        }
    };

    tracing::info!(
        action = "write_analysis_results",
        case_id = %case_id,
        tool_id = %actor.tool_id,
        role = ?actor.role,
        outcome = "allow",
        path = %target,
    );

    Ok(Json(WriteResultResponse {
        ok: true,
        path: target,
    }))
}

async fn append_provenance_event(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
    headers: HeaderMap,
    Json(payload): Json<ProvenanceEventRequest>,
) -> Result<Json<AppendProvenanceResponse>, ApiError> {
    let actor = match actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()) {
        Ok(actor) => actor,
        Err(err) => {
            log_denied_write_best_effort(
                &state,
                &case_id,
                "append_provenance_event",
                &err.message,
                None,
            );
            return Err(err);
        }
    };
    if !actor.role.can_append_provenance() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "append_provenance_event",
            "role cannot append provenance",
            Some(&actor),
        );
        tracing::warn!(
            action = "append_provenance_event",
            case_id = %case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "deny",
            reason = "role cannot append provenance"
        );
        return Err(ApiError::forbidden("role not allowed to append provenance"));
    }
    if let Err(err) = enforce_tool_registry(&state, &actor, WriteLayer::Provenance) {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "append_provenance_event",
            &err.message,
            Some(&actor),
        );
        return Err(err);
    }

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let appended = append_provenance(
        &case_ref,
        &payload.action,
        &payload.actor,
        payload.details,
        payload.tool_name,
        payload.tool_version,
    )?;

    tracing::info!(
        action = "append_provenance_event",
        case_id = %case_id,
        tool_id = %actor.tool_id,
        role = ?actor.role,
        outcome = "allow",
        event_id = %appended.event_id,
    );

    Ok(Json(AppendProvenanceResponse {
        ok: true,
        event_id: appended.event_id,
        timestamp: appended.timestamp,
    }))
}

async fn append_analysis_correction(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
    headers: HeaderMap,
    Json(payload): Json<AnalysisCorrectionRequest>,
) -> Result<Json<AppendAnalysisCorrectionJsonResponse>, ApiError> {
    let actor = match actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()) {
        Ok(actor) => actor,
        Err(err) => {
            log_denied_write_best_effort(
                &state,
                &case_id,
                "append_analysis_correction",
                &err.message,
                None,
            );
            return Err(err);
        }
    };
    if !actor.role.can_append_analysis_correction() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "append_analysis_correction",
            "role cannot append analysis correction",
            Some(&actor),
        );
        tracing::warn!(
            action = "append_analysis_correction",
            case_id = %case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "deny",
            reason = "role cannot append analysis correction"
        );
        return Err(ApiError::forbidden(
            "role not allowed to append analysis correction",
        ));
    }
    if let Err(err) = enforce_tool_registry(&state, &actor, WriteLayer::Analysis) {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "append_analysis_correction",
            &err.message,
            Some(&actor),
        );
        return Err(err);
    }

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let appended = append_analysis_correction_event(
        &case_ref,
        &payload.actor,
        &payload.correction_of,
        &payload.correction_type,
        &payload.reason,
        payload.provenance_ref,
    )?;

    tracing::info!(
        action = "append_analysis_correction",
        case_id = %case_id,
        tool_id = %actor.tool_id,
        role = ?actor.role,
        outcome = "allow",
        event_id = %appended.event_id,
        path = %appended.path,
    );

    Ok(Json(AppendAnalysisCorrectionJsonResponse {
        ok: true,
        event_id: appended.event_id,
        timestamp: appended.timestamp,
        path: appended.path,
    }))
}

// ── Object-producing worker REST handlers (Sprint 11) ─────────────────────────

async fn rest_write_object_delta(
    State(state): State<AppState>,
    AxPath((case_id, job_id)): AxPath<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<ObjectDeltaRequest>,
) -> Result<Json<ObjectDeltaResponse>, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_write_best_effort(&state, &case_id, "write_object_delta", &err.message, None);
    })?;
    if !actor.role.can_write_analysis() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_delta",
            "role cannot write analysis",
            Some(&actor),
        );
        return Err(ApiError::forbidden(
            "role not allowed to write analysis layer",
        ));
    }
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    enforce_capability(&state, &actor, "may_produce_objects").inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let (path, sha256) =
        write_object_delta_rows(&case_ref, &job_id, "objects_delta", &payload.rows)?;
    tracing::info!(action = "write_object_delta", case_id = %case_id, tool_id = %actor.tool_id, outcome = "allow", path = %path);
    Ok(Json(ObjectDeltaResponse {
        ok: true,
        path,
        sha256,
    }))
}

async fn rest_write_object_edge_delta(
    State(state): State<AppState>,
    AxPath((case_id, job_id)): AxPath<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<ObjectDeltaRequest>,
) -> Result<Json<ObjectDeltaResponse>, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_edge_delta",
            &err.message,
            None,
        );
    })?;
    if !actor.role.can_write_analysis() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_edge_delta",
            "role cannot write analysis",
            Some(&actor),
        );
        return Err(ApiError::forbidden(
            "role not allowed to write analysis layer",
        ));
    }
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_edge_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    enforce_capability(&state, &actor, "may_produce_edges").inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_object_edge_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let (path, sha256) =
        write_object_delta_rows(&case_ref, &job_id, "object_edges_delta", &payload.rows)?;
    tracing::info!(action = "write_object_edge_delta", case_id = %case_id, tool_id = %actor.tool_id, outcome = "allow", path = %path);
    Ok(Json(ObjectDeltaResponse {
        ok: true,
        path,
        sha256,
    }))
}

async fn rest_write_derivation_delta(
    State(state): State<AppState>,
    AxPath((case_id, job_id)): AxPath<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<ObjectDeltaRequest>,
) -> Result<Json<ObjectDeltaResponse>, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_derivation_delta",
            &err.message,
            None,
        );
    })?;
    if !actor.role.can_write_analysis() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_derivation_delta",
            "role cannot write analysis",
            Some(&actor),
        );
        return Err(ApiError::forbidden(
            "role not allowed to write analysis layer",
        ));
    }
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_derivation_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    enforce_capability(&state, &actor, "may_produce_derivations").inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_derivation_delta",
            &err.message,
            Some(&actor),
        );
    })?;
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let (path, sha256) =
        write_object_delta_rows(&case_ref, &job_id, "derivations_delta", &payload.rows)?;
    tracing::info!(action = "write_derivation_delta", case_id = %case_id, tool_id = %actor.tool_id, outcome = "allow", path = %path);
    Ok(Json(ObjectDeltaResponse {
        ok: true,
        path,
        sha256,
    }))
}

async fn rest_write_materialized_object(
    State(state): State<AppState>,
    AxPath((case_id, job_id)): AxPath<(String, String)>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Json<WriteMaterializedObjectRestResponse>, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_materialized_object",
            &err.message,
            None,
        );
    })?;
    if !actor.role.can_write_analysis() {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_materialized_object",
            "role cannot write analysis",
            Some(&actor),
        );
        return Err(ApiError::forbidden(
            "role not allowed to write analysis layer",
        ));
    }
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis).inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_materialized_object",
            &err.message,
            Some(&actor),
        );
    })?;
    enforce_capability(&state, &actor, "may_materialize_objects").inspect_err(|err| {
        log_denied_write_best_effort(
            &state,
            &case_id,
            "write_materialized_object",
            &err.message,
            Some(&actor),
        );
    })?;
    let _ = normalize_job_id(&job_id)?;
    let case_ref = resolve_case_ref(&state, &case_id)?;
    let sha256 = write_derived_object(&case_ref, &body).map_err(ApiError::from)?;
    tracing::info!(action = "write_materialized_object", case_id = %case_id, tool_id = %actor.tool_id, sha256 = %sha256, outcome = "allow");
    Ok(Json(WriteMaterializedObjectRestResponse {
        ok: true,
        sha256,
    }))
}

async fn rest_get_objects(
    State(state): State<AppState>,
    AxPath(case_id): AxPath<String>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    Ok(Json(read_object_index_values(&case_ref)?))
}

async fn rest_get_object_children(
    State(state): State<AppState>,
    AxPath((case_id, object_id)): AxPath<(String, String)>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    Ok(Json(read_object_children(&case_ref, &object_id)?))
}

async fn rest_get_object_parents(
    State(state): State<AppState>,
    AxPath((case_id, object_id)): AxPath<(String, String)>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    Ok(Json(read_object_parents(&case_ref, &object_id)?))
}

async fn rest_get_object_lineage(
    State(state): State<AppState>,
    AxPath((case_id, object_id)): AxPath<(String, String)>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let case_ref = resolve_case_ref(&state, &case_id)?;
    Ok(Json(read_object_lineage(&case_ref, &object_id)?))
}

async fn rest_get_object_content(
    State(state): State<AppState>,
    AxPath((case_id, object_id)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> Result<AxumResponse, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_access_best_effort(
            &state,
            &case_id,
            "input_read_denied",
            "object",
            &object_id,
            &err.message,
            None,
        );
    })?;
    enforce_tool_registry_read(&state, &actor).inspect_err(|err| {
        log_denied_access_best_effort(
            &state,
            &case_id,
            "input_read_denied",
            "object",
            &object_id,
            &err.message,
            Some(&actor),
        );
    })?;

    let scope_ref = header_str(&headers, "x-offf-scope-ref");
    let policy_refs = header_csv(&headers, "x-offf-policy-ref");

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let content = match &case_ref {
        ContainerRef::Local(base) => read_object_verified(base, &object_id).map_err(ApiError::from)?,
        ContainerRef::S3 { .. } => {
            return Err(ApiError::bad_request(
                "verified object read endpoint currently supports local containers only",
            ));
        }
    };

    log_allowed_access_best_effort(
        &state,
        &case_id,
        "input_read_allowed",
        "object",
        &object_id,
        &actor,
        scope_ref.as_deref(),
        &policy_refs,
    );

    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        content,
    )
        .into_response())
}

async fn rest_get_file_content(
    State(state): State<AppState>,
    AxPath((case_id, filesystem_id, file_id)): AxPath<(String, String, String)>,
    headers: HeaderMap,
) -> Result<AxumResponse, ApiError> {
    let actor = actor_from_headers(state.auth_mode, &headers, state.jwt_secret.as_deref()).inspect_err(|err| {
        log_denied_access_best_effort(
            &state,
            &case_id,
            "input_read_denied",
            "file",
            &format!("{filesystem_id}:{file_id}"),
            &err.message,
            None,
        );
    })?;
    enforce_tool_registry_read(&state, &actor).inspect_err(|err| {
        log_denied_access_best_effort(
            &state,
            &case_id,
            "input_read_denied",
            "file",
            &format!("{filesystem_id}:{file_id}"),
            &err.message,
            Some(&actor),
        );
    })?;

    let scope_ref = header_str(&headers, "x-offf-scope-ref");
    let policy_refs = header_csv(&headers, "x-offf-policy-ref");

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let content = match &case_ref {
        ContainerRef::Local(base) => {
            read_file_verified(base, &filesystem_id, &file_id).map_err(ApiError::from)?
        }
        ContainerRef::S3 { .. } => {
            return Err(ApiError::bad_request(
                "verified file read endpoint currently supports local containers only",
            ));
        }
    };

    log_allowed_access_best_effort(
        &state,
        &case_id,
        "input_read_allowed",
        "file",
        &format!("{filesystem_id}:{file_id}"),
        &actor,
        scope_ref.as_deref(),
        &policy_refs,
    );

    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        content,
    )
        .into_response())
}

// ── Object-producing helpers ──────────────────────────────────────────────────

fn normalize_job_id(job_id: &str) -> Result<String, ApiError> {
    let j = job_id.trim();
    if j.is_empty() || j.contains('/') || j.contains("..") || j.contains('\\') {
        return Err(ApiError::bad_request("invalid job_id"));
    }
    Ok(j.to_string())
}

fn write_object_delta_rows(
    case_ref: &ContainerRef,
    job_id: &str,
    artifact: &str,
    rows: &[Value],
) -> Result<(String, String), ApiError> {
    // T-10: guard against oversized batch writes
    if rows.len() > MAX_ROWS_PER_REQUEST {
        return Err(ApiError::bad_request(format!(
            "rows.len() {} exceeds maximum {MAX_ROWS_PER_REQUEST}",
            rows.len()
        )));
    }
    let job_id_clean = normalize_job_id(job_id)?;
    let rel = format!("analysis/jobs/{job_id_clean}/{artifact}.jsonl");
    if case_ref.exists(&rel).map_err(ApiError::from)? {
        return Err(ApiError::forbidden(format!(
            "refusing to overwrite existing artifact: {rel}"
        )));
    }
    let mut content = String::new();
    for row in rows {
        content.push_str(&serde_json::to_string(row)?);
        content.push('\n');
    }
    let data = content.as_bytes();
    let sha256 = format!("sha256:{}", hex_sha256(data));
    case_ref.write_bytes(&rel, data).map_err(ApiError::from)?;
    Ok((rel, sha256))
}

fn enforce_capability(
    state: &AppState,
    actor: &ActorContext,
    capability: &str,
) -> Result<(), ApiError> {
    let registry = load_tool_registry(Path::new(&state.tool_registry_path))?;
    let rec = registry
        .tools
        .iter()
        .find(|t| t.tool_id == actor.tool_id)
        .ok_or_else(|| ApiError::forbidden(format!("tool not registered: {}", actor.tool_id)))?;
    if !rec
        .capabilities
        .iter()
        .any(|c| c.eq_ignore_ascii_case(capability))
    {
        return Err(ApiError::forbidden(format!(
            "tool does not have capability {capability}: {}",
            actor.tool_id
        )));
    }
    Ok(())
}

fn read_object_index_values(case_ref: &ContainerRef) -> Result<Vec<Value>, ApiError> {
    let rel = "indexes/objects/object_index.parquet";
    if !case_ref.exists(rel).map_err(ApiError::from)? {
        return Ok(vec![]);
    }
    let data = case_ref.read_bytes(rel).map_err(ApiError::from)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(data))?;
    let reader = builder.build()?;
    let mut out = Vec::new();
    for batch in reader {
        out.extend(batch_to_json_rows(&batch?)?);
    }
    Ok(out)
}

fn read_object_edges_values(case_ref: &ContainerRef) -> Result<Vec<Value>, ApiError> {
    let rel = "indexes/objects/object_edges.parquet";
    if !case_ref.exists(rel).map_err(ApiError::from)? {
        return Ok(vec![]);
    }
    let data = case_ref.read_bytes(rel).map_err(ApiError::from)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(data))?;
    let reader = builder.build()?;
    let mut out = Vec::new();
    for batch in reader {
        out.extend(batch_to_json_rows(&batch?)?);
    }
    Ok(out)
}

fn read_object_children(case_ref: &ContainerRef, object_id: &str) -> Result<Vec<Value>, ApiError> {
    let edges = read_object_edges_values(case_ref)?;
    Ok(edges
        .into_iter()
        .filter(|e| {
            e.get("parent_object_id")
                .and_then(|v| v.as_str())
                .map(|id| id == object_id)
                .unwrap_or(false)
        })
        .collect())
}

fn read_object_parents(case_ref: &ContainerRef, object_id: &str) -> Result<Vec<Value>, ApiError> {
    let edges = read_object_edges_values(case_ref)?;
    Ok(edges
        .into_iter()
        .filter(|e| {
            e.get("child_object_id")
                .and_then(|v| v.as_str())
                .map(|id| id == object_id)
                .unwrap_or(false)
        })
        .collect())
}

fn read_object_lineage(case_ref: &ContainerRef, object_id: &str) -> Result<Vec<Value>, ApiError> {
    let edges = read_object_edges_values(case_ref)?;
    let mut lineage: Vec<Value> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(object_id.to_string());
    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }
        for edge in &edges {
            let child = edge
                .get("child_object_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let parent = edge
                .get("parent_object_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if child == current && !visited.contains(parent) {
                lineage.push(edge.clone());
                queue.push_back(parent.to_string());
            }
        }
    }
    Ok(lineage)
}

fn list_file_index_values(
    case_ref: &ContainerRef,
    partition_id: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let mut rows = Vec::new();

    if let Some(partition_id) = partition_id {
        let rel = format!("indexes/filesystems/{partition_id}/file_index.parquet");
        if case_ref.exists(&rel).map_err(ApiError::from)? {
            rows.extend(read_file_index_rows(case_ref, &rel)?);
        }
    } else {
        for rel in case_ref
            .list_relative_keys("indexes/filesystems/")
            .map_err(ApiError::from)?
        {
            if rel.ends_with("/file_index.parquet") {
                rows.extend(read_file_index_rows(case_ref, &rel)?);
            }
        }
    }

    Ok(rows)
}

fn write_analysis_rows(
    case_ref: &ContainerRef,
    relative_path: &str,
    rows: &[Value],
) -> Result<String, ApiError> {
    // T-10: guard against oversized batch writes
    if rows.len() > MAX_ROWS_PER_REQUEST {
        return Err(ApiError::bad_request(format!(
            "rows.len() {} exceeds maximum {MAX_ROWS_PER_REQUEST}",
            rows.len()
        )));
    }
    let rel = normalize_rel_path(relative_path)?;
    if !rel.starts_with("analysis/jobs/") {
        return Err(ApiError::bad_request(
            "relative_path must start with analysis/jobs/",
        ));
    }
    if rel.starts_with("indexes/") {
        return Err(ApiError::bad_request(
            "direct writes to indexes/ are denied; use the object-delta endpoints",
        ));
    }
    if !rel.ends_with(".jsonl") {
        return Err(ApiError::bad_request("only .jsonl is currently supported"));
    }

    let segments: Vec<&str> = rel.split('/').collect();
    if segments.len() < 4 || segments[2].trim().is_empty() {
        return Err(ApiError::bad_request(
            "relative_path must include job directory: analysis/jobs/{job_id}/...",
        ));
    }

    if case_ref.exists(&rel).map_err(ApiError::from)? {
        return Err(ApiError::forbidden(
            "refusing to overwrite existing analysis result",
        ));
    }

    let mut content = String::new();
    for row in rows {
        content.push_str(&serde_json::to_string(row)?);
        content.push('\n');
    }
    case_ref
        .write_bytes(&rel, content.as_bytes())
        .map_err(ApiError::from)?;

    Ok(rel)
}

/// T-04/T-03/T-09: Validate a HMAC-HS256 bearer token and extract actor claims.
///
/// Token format: `<base64url_json_payload>.<base64url_hmac_sha256_signature>`
/// Payload JSON: `{"tool_id":"...","role":"...","exp":<unix_epoch_secs>}`
///
/// Returns `(tool_id, role)` on success, or an `ApiError::unauthorized` on any failure.
fn validate_hmac_token(token: &str, secret: &[u8]) -> Result<(String, String), ApiError> {
    let invalid = || ApiError::unauthorized("invalid or expired token");

    let (payload_b64, sig_b64) = token.split_once('.').ok_or_else(invalid)?;

    // Verify HMAC-SHA256 signature over the payload segment.
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .map_err(|_| ApiError::unauthorized("token verification error"))?;
    mac.update(payload_b64.as_bytes());
    let expected_sig = mac.finalize().into_bytes();
    let provided_sig = Base64UrlUnpadded::decode_vec(sig_b64).map_err(|_| invalid())?;
    // Constant-time comparison to prevent timing attacks.
    if expected_sig.len() != provided_sig.len() {
        return Err(invalid());
    }
    use std::ops::BitXor;
    let mismatch = expected_sig
        .iter()
        .zip(provided_sig.iter())
        .fold(0u8, |acc, (a, b)| acc | a.bitxor(b));
    if mismatch != 0 {
        return Err(invalid());
    }

    // Decode payload.
    let payload_bytes = Base64UrlUnpadded::decode_vec(payload_b64).map_err(|_| invalid())?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).map_err(|_| invalid())?;

    // Validate expiry.
    let exp = payload
        .get("exp")
        .and_then(|v| v.as_i64())
        .ok_or_else(invalid)?;
    let now = Utc::now().timestamp();
    if now > exp {
        return Err(ApiError::unauthorized("token expired"));
    }

    let tool_id = payload
        .get("tool_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(invalid)?
        .to_string();
    let role = payload
        .get("role")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(invalid)?
        .to_string();

    Ok((tool_id, role))
}

fn actor_from_headers(auth_mode: AuthMode, headers: &HeaderMap, jwt_secret: Option<&[u8]>) -> Result<ActorContext, ApiError> {
    // T-04: In JWT mode, validate the signed bearer token instead of trusting raw headers.
    if matches!(auth_mode, AuthMode::Jwt) {
        let secret = jwt_secret.ok_or_else(|| ApiError::unauthorized("jwt mode not configured"))?;
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("missing Authorization: Bearer <token>"))?;
        let (tool_id, role_str) = validate_hmac_token(bearer, secret)?;
        let role = AppRole::parse(&role_str)
            .ok_or_else(|| ApiError::unauthorized("invalid role in token"))?;
        return Ok(ActorContext { role, tool_id });
    }

    let role_raw = headers
        .get(auth_mode.role_header())
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::unauthorized(format!(
                "missing {} for auth mode {}",
                auth_mode.role_header(),
                auth_mode.as_str()
            ))
        })?;
    let tool_id = headers
        .get(auth_mode.tool_header())
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::unauthorized(format!(
                "missing {} for auth mode {}",
                auth_mode.tool_header(),
                auth_mode.as_str()
            ))
        })?
        .trim()
        .to_string();
    if tool_id.is_empty() {
        return Err(ApiError::unauthorized("empty x-offf-tool-id"));
    }

    let role =
        AppRole::parse(role_raw).ok_or_else(|| ApiError::unauthorized("invalid x-offf-role"))?;

    Ok(ActorContext { role, tool_id })
}

fn actor_from_metadata(
    auth_mode: AuthMode,
    metadata: &tonic::metadata::MetadataMap,
    jwt_secret: Option<&[u8]>,
) -> Result<ActorContext, AuthError> {
    // T-04: In JWT mode, validate the signed bearer token from the gRPC Authorization metadata.
    if matches!(auth_mode, AuthMode::Jwt) {
        let secret = jwt_secret
            .ok_or_else(|| AuthError::Unauthorized("jwt mode not configured".to_string()))?;
        let bearer = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                AuthError::Unauthorized("missing authorization: Bearer <token>".to_string())
            })?;
        let (tool_id, role_str) = validate_hmac_token(bearer, secret)
            .map_err(|e| AuthError::Unauthorized(e.message))?;
        let role = AppRole::parse(&role_str)
            .ok_or_else(|| AuthError::Unauthorized("invalid role in token".to_string()))?;
        return Ok(ActorContext { role, tool_id });
    }

    let role_raw = metadata
        .get(auth_mode.role_header())
        .ok_or_else(|| {
            AuthError::Unauthorized(format!(
                "missing {} for auth mode {}",
                auth_mode.role_header(),
                auth_mode.as_str()
            ))
        })?
        .to_str()
        .map_err(|_| AuthError::Unauthorized(format!("invalid {}", auth_mode.role_header())))?;
    let tool_id = metadata
        .get(auth_mode.tool_header())
        .ok_or_else(|| {
            AuthError::Unauthorized(format!(
                "missing {} for auth mode {}",
                auth_mode.tool_header(),
                auth_mode.as_str()
            ))
        })?
        .to_str()
        .map_err(|_| AuthError::Unauthorized(format!("invalid {}", auth_mode.tool_header())))?
        .trim()
        .to_string();
    if tool_id.is_empty() {
        return Err(AuthError::Unauthorized("empty x-offf-tool-id".to_string()));
    }

    let role = AppRole::parse(role_raw)
        .ok_or_else(|| AuthError::Unauthorized("invalid x-offf-role".to_string()))?;

    Ok(ActorContext { role, tool_id })
}

fn grpc_status_from_auth(err: AuthError) -> Status {
    match err {
        AuthError::Unauthorized(msg) => Status::new(Code::Unauthenticated, msg),
    }
}

fn enforce_tool_registry(
    state: &AppState,
    actor: &ActorContext,
    layer: WriteLayer,
) -> Result<(), ApiError> {
    let registry = load_tool_registry(Path::new(&state.tool_registry_path))?;
    let rec = registry
        .tools
        .iter()
        .find(|t| t.tool_id == actor.tool_id)
        .ok_or_else(|| ApiError::forbidden(format!("tool not registered: {}", actor.tool_id)))?;

    if !rec.status.eq_ignore_ascii_case("approved") {
        return Err(ApiError::forbidden(format!(
            "tool is not approved: {}",
            actor.tool_id
        )));
    }

    let role_name = role_name(actor.role);
    if !rec
        .allowed_roles
        .iter()
        .any(|r| r.eq_ignore_ascii_case(role_name))
    {
        return Err(ApiError::forbidden(format!(
            "tool role not allowed: tool={} role={role_name}",
            actor.tool_id
        )));
    }

    if !rec
        .write_layers
        .iter()
        .any(|l| l.eq_ignore_ascii_case(layer.as_str()))
    {
        return Err(ApiError::forbidden(format!(
            "tool write layer not allowed: tool={} layer={}",
            actor.tool_id,
            layer.as_str()
        )));
    }

    Ok(())
}

fn enforce_tool_registry_read(state: &AppState, actor: &ActorContext) -> Result<(), ApiError> {
    let registry = load_tool_registry(Path::new(&state.tool_registry_path))?;
    let rec = registry
        .tools
        .iter()
        .find(|t| t.tool_id == actor.tool_id)
        .ok_or_else(|| ApiError::forbidden(format!("tool not registered: {}", actor.tool_id)))?;

    if !rec.status.eq_ignore_ascii_case("approved") {
        return Err(ApiError::forbidden(format!(
            "tool is not approved: {}",
            actor.tool_id
        )));
    }

    let role_name = role_name(actor.role);
    if !rec
        .allowed_roles
        .iter()
        .any(|r| r.eq_ignore_ascii_case(role_name))
    {
        return Err(ApiError::forbidden(format!(
            "tool role not allowed: tool={} role={role_name}",
            actor.tool_id
        )));
    }

    Ok(())
}

fn load_tool_registry(path: &Path) -> Result<ToolRegistryFile, ApiError> {
    if !path.exists() {
        return Err(ApiError::forbidden(format!(
            "tool registry not found: {}",
            path.to_string_lossy()
        )));
    }
    let raw = fs::read_to_string(path).map_err(ApiError::from)?;
    serde_json::from_str(&raw).map_err(ApiError::from)
}

fn role_name(role: AppRole) -> &'static str {
    match role {
        AppRole::Viewer => "viewer",
        AppRole::Reporter => "reporter",
        AppRole::AnalysisWorker => "analysis_worker",
        AppRole::Indexer => "indexer",
        AppRole::AcquisitionProducer => "acquisition_producer",
    }
}

struct AppendedProvenance {
    event_id: String,
    timestamp: String,
}

struct AppendedAnalysisEvent {
    event_id: String,
    timestamp: String,
    path: String,
}

fn append_analysis_correction_event(
    case_ref: &ContainerRef,
    actor: &str,
    correction_of: &str,
    correction_type: &str,
    reason: &str,
    provenance_ref: Option<String>,
) -> Result<AppendedAnalysisEvent, ApiError> {
    if actor.trim().is_empty() {
        return Err(ApiError::bad_request("actor must not be empty"));
    }
    if correction_of.trim().is_empty() {
        return Err(ApiError::bad_request("correction_of must not be empty"));
    }
    if correction_type.trim().is_empty() {
        return Err(ApiError::bad_request("correction_type must not be empty"));
    }
    if reason.trim().is_empty() {
        return Err(ApiError::bad_request("reason must not be empty"));
    }

    let rel = "analysis/events/analysis_events.jsonl";
    let counter = case_ref
        .read_text(rel)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
        + 1;
    let timestamp = Utc::now().to_rfc3339();

    let mut event = serde_json::Map::new();
    event.insert(
        "event_id".to_string(),
        Value::String(format!("analysis-correction-{counter:06}")),
    );
    event.insert("timestamp".to_string(), Value::String(timestamp.clone()));
    event.insert("actor".to_string(), Value::String(actor.trim().to_string()));
    event.insert(
        "correction_of".to_string(),
        Value::String(correction_of.trim().to_string()),
    );
    event.insert(
        "correction_type".to_string(),
        Value::String(correction_type.trim().to_string()),
    );
    event.insert(
        "reason".to_string(),
        Value::String(reason.trim().to_string()),
    );
    if let Some(prov) = provenance_ref.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        event.insert("provenance_ref".to_string(), Value::String(prov));
    }

    let event_value = Value::Object(event);
    case_ref
        .append_jsonl_line(rel, &serde_json::to_string(&event_value)?)
        .map_err(ApiError::from)?;

    Ok(AppendedAnalysisEvent {
        event_id: event_value
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        timestamp,
        path: rel.to_string(),
    })
}

fn append_provenance(
    case_ref: &ContainerRef,
    action: &str,
    actor: &str,
    details: Value,
    tool_name: Option<String>,
    tool_version: Option<String>,
) -> Result<AppendedProvenance, ApiError> {
    let tool_name = tool_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "offf-access-service".to_string());
    let tool_version = tool_version
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "0.1.0".to_string());
    let rel = "provenance/chain_of_custody.jsonl";
    let counter = case_ref
        .read_text(rel)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();

    let event = serde_json::json!({
        "event_id": format!("evt-{counter:06}"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "actor": actor,
        "action": action,
        "tool": ToolInfo {
            name: tool_name,
            version: tool_version,
        },
        "details": details,
    });
    case_ref
        .append_jsonl_line(rel, &serde_json::to_string(&event)?)
        .map_err(ApiError::from)?;

    Ok(AppendedProvenance {
        event_id: event
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        timestamp: event
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

fn log_denied_write_best_effort(
    state: &AppState,
    case_id: &str,
    action: &str,
    reason: &str,
    actor: Option<&ActorContext>,
) {
    let case_ref = match resolve_case_ref(state, case_id) {
        Ok(case_ref) => case_ref,
        Err(err) => {
            tracing::warn!(
                action = action,
                case_id = %case_id,
                outcome = "deny_log_skip",
                reason = %reason,
                resolve_error = %err.message,
            );
            return;
        }
    };

    let counter = case_ref
        .read_text(DENIED_WRITES_REL)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
        + 1;

    let mut event = serde_json::Map::new();
    event.insert(
        "event_id".to_string(),
        Value::String(format!("denied-{counter:06}")),
    );
    event.insert(
        "timestamp".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );
    event.insert("case_id".to_string(), Value::String(case_id.to_string()));
    event.insert("action".to_string(), Value::String(action.to_string()));
    event.insert(
        "auth_mode".to_string(),
        Value::String(state.auth_mode.as_str().to_string()),
    );
    event.insert("reason".to_string(), Value::String(reason.to_string()));

    if let Some(actor) = actor {
        event.insert("tool_id".to_string(), Value::String(actor.tool_id.clone()));
        event.insert(
            "role".to_string(),
            Value::String(role_name(actor.role).to_string()),
        );
    }

    let line = Value::Object(event);
    match serde_json::to_string(&line) {
        Ok(jsonl) => {
            if let Err(err) = case_ref.append_jsonl_line(DENIED_WRITES_REL, &jsonl) {
                tracing::warn!(
                    action = action,
                    case_id = %case_id,
                    outcome = "deny_log_failed",
                    reason = %reason,
                    error = %err,
                );
            }
        }
        Err(err) => {
            tracing::warn!(
                action = action,
                case_id = %case_id,
                outcome = "deny_log_failed",
                reason = %reason,
                error = %err,
            );
        }
    }
}

fn log_allowed_access_best_effort(
    state: &AppState,
    case_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    actor: &ActorContext,
    scope_ref: Option<&str>,
    policy_refs: &[String],
) {
    let case_ref = match resolve_case_ref(state, case_id) {
        Ok(case_ref) => case_ref,
        Err(err) => {
            tracing::warn!(
                action = action,
                case_id = %case_id,
                outcome = "allow_log_skip",
                resolve_error = %err.message,
            );
            return;
        }
    };

    let counter = case_ref
        .read_text(ACCESS_EVENTS_REL)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
        + 1;

    let event = serde_json::json!({
        "access_event_id": format!("access-{counter:06}"),
        "timestamp": Utc::now().to_rfc3339(),
        "actor": actor.tool_id,
        "tool": {
            "name": ACCESS_TOOL_NAME,
            "version": ACCESS_TOOL_VERSION,
        },
        "action": action,
        "target": {
            "type": target_type,
            "id": target_id,
        },
        "scope_ref": scope_ref,
        "policy_refs": policy_refs,
        "result": "allowed"
    });

    if let Ok(jsonl) = serde_json::to_string(&event) {
        let _ = case_ref.append_jsonl_line(ACCESS_EVENTS_REL, &jsonl);
    }
}

fn log_denied_access_best_effort(
    state: &AppState,
    case_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    reason: &str,
    actor: Option<&ActorContext>,
) {
    let case_ref = match resolve_case_ref(state, case_id) {
        Ok(case_ref) => case_ref,
        Err(_) => return,
    };

    let counter = case_ref
        .read_text(ACCESS_DENIED_REL)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
        + 1;

    let actor_id = actor.map(|a| a.tool_id.clone()).unwrap_or_else(|| "unknown".to_string());
    let event = serde_json::json!({
        "denied_event_id": format!("denied-read-{counter:06}"),
        "timestamp": Utc::now().to_rfc3339(),
        "actor": actor_id,
        "tool": {
            "name": ACCESS_TOOL_NAME,
            "version": ACCESS_TOOL_VERSION,
        },
        "action": action,
        "target": {
            "type": target_type,
            "id": target_id,
        },
        "result": "denied",
        "reason_code": reason,
        "scope_ref": serde_json::Value::Null,
        "policy_refs": []
    });

    if let Ok(jsonl) = serde_json::to_string(&event) {
        let _ = case_ref.append_jsonl_line(ACCESS_DENIED_REL, &jsonl);
    }
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

fn header_csv(headers: &HeaderMap, name: &str) -> Vec<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn value_to_file_row(row: &Value) -> FileRow {
    let get_str = |k: &str| {
        row.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let get_u64 = |k: &str| row.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    let get_bool = |k: &str| row.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

    FileRow {
        file_id: get_u64("file_id"),
        filesystem_id: get_str("filesystem_id"),
        partition_id: get_str("partition_id"),
        path: get_str("path"),
        filename: get_str("filename"),
        extension: get_str("extension"),
        size_bytes: get_u64("size_bytes"),
        is_directory: get_bool("is_directory"),
        is_deleted: get_bool("is_deleted"),
    }
}

fn grpc_status(err: ApiError) -> Status {
    let code = match err.status {
        StatusCode::BAD_REQUEST => Code::InvalidArgument,
        StatusCode::NOT_FOUND => Code::NotFound,
        StatusCode::UNAUTHORIZED => Code::Unauthenticated,
        StatusCode::FORBIDDEN => Code::PermissionDenied,
        _ => Code::Internal,
    };
    Status::new(code, err.message)
}

fn resolve_case_ref(state: &AppState, case_id: &str) -> Result<ContainerRef, ApiError> {
    if case_id.starts_with("s3://") {
        let case_ref = ContainerRef::parse(case_id).map_err(ApiError::from)?;
        if case_ref.exists("manifest.json").map_err(ApiError::from)? {
            return Ok(case_ref);
        }
        return Err(ApiError::not_found(format!("case not found: {case_id}")));
    }

    if state.cases_root.starts_with("s3://") {
        let root = state.cases_root.trim_end_matches('/');
        for candidate in [
            format!("{root}/{case_id}"),
            format!("{root}/{case_id}.offf"),
        ] {
            let case_ref = ContainerRef::parse(&candidate).map_err(ApiError::from)?;
            if case_ref.exists("manifest.json").map_err(ApiError::from)? {
                return Ok(case_ref);
            }
        }
        return Err(ApiError::not_found(format!("case not found: {case_id}")));
    }

    for candidate in [
        std::path::PathBuf::from(&state.cases_root).join(case_id),
        std::path::PathBuf::from(&state.cases_root).join(format!("{case_id}.offf")),
    ] {
        if candidate.exists() {
            return Ok(ContainerRef::Local(candidate));
        }
    }
    Err(ApiError::not_found(format!("case not found: {case_id}")))
}

fn load_case_data(
    state: &AppState,
    case_id: &str,
) -> Result<(ContainerRef, ManifestJson, Vec<ChunkMetadata>), ApiError> {
    let case_ref = resolve_case_ref(state, case_id)?;
    let manifest_value = read_json_file(&case_ref, "manifest.json")?;
    let manifest: ManifestJson = serde_json::from_value(manifest_value)?;
    let map_data = case_ref
        .read_bytes(manifest.indexes.physical_to_chunk.as_deref().unwrap_or("maps/physical_to_chunk.parquet"))
        .map_err(ApiError::from)?;
    let chunks = read_physical_to_chunk_bytes(&map_data).map_err(ApiError::from)?;
    Ok((case_ref, manifest, chunks))
}

fn read_json_file(case_ref: &ContainerRef, rel: &str) -> Result<Value, ApiError> {
    let raw = case_ref.read_text(rel).map_err(ApiError::from)?;
    let value: Value = serde_json::from_str(&raw)?;
    Ok(value)
}

fn find_chunk<'a>(
    chunks: &'a [ChunkMetadata],
    chunk_id: &str,
) -> Result<&'a ChunkMetadata, ApiError> {
    chunks
        .iter()
        .find(|c| c.chunk_id == chunk_id)
        .ok_or_else(|| ApiError::not_found(format!("chunk not found: {chunk_id}")))
}

fn normalize_rel_path(path: &str) -> Result<String, ApiError> {
    let rel = path.replace('\\', "/").trim_start_matches('/').to_string();
    if rel.contains("..") {
        return Err(ApiError::bad_request("path traversal is not allowed"));
    }
    Ok(rel)
}

fn read_file_index_rows(case_ref: &ContainerRef, rel: &str) -> Result<Vec<Value>, ApiError> {
    let data = case_ref.read_bytes(rel).map_err(ApiError::from)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(data))?;
    let reader = builder.build()?;

    let mut out = Vec::new();
    for batch in reader {
        let batch = batch?;
        out.extend(batch_to_json_rows(&batch)?);
    }
    Ok(out)
}

fn list_analysis_artifacts(case_ref: &ContainerRef) -> Result<Vec<String>, ApiError> {
    let mut out = case_ref
        .list_relative_keys("analysis/")
        .map_err(ApiError::from)?;
    out.sort();
    Ok(out)
}

fn batch_to_json_rows(batch: &RecordBatch) -> Result<Vec<Value>, ApiError> {
    let mut rows = Vec::with_capacity(batch.num_rows());
    for i in 0..batch.num_rows() {
        let mut obj = serde_json::Map::new();
        for (col_idx, field) in batch.schema().fields().iter().enumerate() {
            let name = field.name().clone();
            let array = batch.column(col_idx);
            let value = if let Some(v) = array.as_any().downcast_ref::<StringArray>() {
                Value::String(v.value(i).to_string())
            } else if let Some(v) = array.as_any().downcast_ref::<UInt64Array>() {
                json!(v.value(i))
            } else if let Some(v) = array.as_any().downcast_ref::<BooleanArray>() {
                json!(v.value(i))
            } else {
                Value::Null
            };
            obj.insert(name, value);
        }
        rows.push(Value::Object(obj));
    }
    Ok(rows)
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }

    fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }

    fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> AxumResponse {
        let body = Json(json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        ApiError::internal(value.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        ApiError::internal(value.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(value: serde_json::Error) -> Self {
        ApiError::bad_request(value.to_string())
    }
}

impl From<offf_core::OfffError> for ApiError {
    fn from(value: offf_core::OfffError) -> Self {
        ApiError::bad_request(value.to_string())
    }
}

impl From<parquet::errors::ParquetError> for ApiError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        ApiError::bad_request(value.to_string())
    }
}

impl From<arrow::error::ArrowError> for ApiError {
    fn from(value: arrow::error::ArrowError) -> Self {
        ApiError::bad_request(value.to_string())
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // T-06: Explicit path traversal negative tests for normalize_rel_path
    #[test]
    fn path_traversal_dot_dot_blocked() {
        let err = normalize_rel_path("analysis/jobs/../../../etc/passwd").unwrap_err();
        assert!(err.message.contains("traversal"));
    }

    #[test]
    fn path_traversal_encoded_dot_dot_blocked() {
        // Encoded ../ should still contain ".." after basic normalisation
        let err = normalize_rel_path("analysis/jobs/..%2F..%2Fetc").unwrap_err();
        assert!(err.message.contains("traversal"));
    }

    #[test]
    fn path_traversal_dot_dot_in_segment_blocked() {
        let err = normalize_rel_path("analysis/..jobs/evil.jsonl").unwrap_err();
        assert!(err.message.contains("traversal"));
    }

    #[test]
    fn path_traversal_backslash_converted() {
        // Backslash is converted to forward-slash, then the path is valid
        let result = normalize_rel_path("analysis\\jobs\\job1\\hits.jsonl").unwrap();
        assert_eq!(result, "analysis/jobs/job1/hits.jsonl");
    }

    #[test]
    fn path_traversal_backslash_with_dot_dot_blocked() {
        let err = normalize_rel_path("analysis\\..\\etc\\passwd").unwrap_err();
        assert!(err.message.contains("traversal"));
    }

    #[test]
    fn path_traversal_leading_slash_stripped() {
        let result = normalize_rel_path("/analysis/jobs/x/hits.jsonl").unwrap();
        assert_eq!(result, "analysis/jobs/x/hits.jsonl");
    }

    #[test]
    fn path_valid_accepted() {
        let result = normalize_rel_path("analysis/jobs/job-123/keyword_hits.jsonl").unwrap();
        assert_eq!(result, "analysis/jobs/job-123/keyword_hits.jsonl");
    }

    // T-04: Unit tests for HMAC token validation
    fn make_token(payload_json: &str, secret: &[u8]) -> String {
        let payload_b64 = Base64UrlUnpadded::encode_string(payload_json.as_bytes());
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(payload_b64.as_bytes());
        let sig = mac.finalize().into_bytes();
        let sig_b64 = Base64UrlUnpadded::encode_string(&sig);
        format!("{payload_b64}.{sig_b64}")
    }

    #[test]
    fn jwt_valid_token_accepted() {
        let secret = b"test-secret-key";
        let exp = Utc::now().timestamp() + 3600;
        let payload = format!(r#"{{"tool_id":"analyzer-1","role":"analysis_worker","exp":{exp}}}"#);
        let token = make_token(&payload, secret);
        let (tool_id, role) = validate_hmac_token(&token, secret).unwrap();
        assert_eq!(tool_id, "analyzer-1");
        assert_eq!(role, "analysis_worker");
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let exp = Utc::now().timestamp() + 3600;
        let payload = format!(r#"{{"tool_id":"t","role":"viewer","exp":{exp}}}"#);
        let token = make_token(&payload, b"correct-secret");
        let err = validate_hmac_token(&token, b"wrong-secret").unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn jwt_expired_token_rejected() {
        let secret = b"test-secret-key";
        let exp = Utc::now().timestamp() - 1; // 1 second in the past
        let payload = format!(r#"{{"tool_id":"t","role":"viewer","exp":{exp}}}"#);
        let token = make_token(&payload, secret);
        let err = validate_hmac_token(&token, secret).unwrap_err();
        assert!(err.message.contains("expired"));
    }

    #[test]
    fn jwt_tampered_payload_rejected() {
        let secret = b"test-secret-key";
        let exp = Utc::now().timestamp() + 3600;
        let payload = format!(r#"{{"tool_id":"t","role":"viewer","exp":{exp}}}"#);
        let token = make_token(&payload, secret);
        // Replace the payload segment with a tampered one
        let sig = token.split('.').nth(1).unwrap();
        let tampered_payload =
            Base64UrlUnpadded::encode_string(b"{\"tool_id\":\"evil\",\"role\":\"admin\",\"exp\":9999999999}");
        let tampered_token = format!("{tampered_payload}.{sig}");
        let err = validate_hmac_token(&tampered_token, secret).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn jwt_missing_dot_separator_rejected() {
        let err = validate_hmac_token("notavalidtoken", b"secret").unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
    }

    // T-10: max rows guard
    #[test]
    fn max_rows_per_request_constant_is_reasonable() {
        assert!(MAX_ROWS_PER_REQUEST >= 1_000);
        assert!(MAX_ROWS_PER_REQUEST <= 1_000_000);
    }
}
