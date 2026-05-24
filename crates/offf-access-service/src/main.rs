use std::{
    env, fs,
    net::SocketAddr,
    path::Path,
};

use anyhow::{Context, Result};
use chrono::Utc;
use arrow::{
    array::{BooleanArray, StringArray, UInt64Array},
    record_batch::RecordBatch,
};
use axum::{
    extract::{Path as AxPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
    Json, Router,
};
use offf_core::{
    parquet_io::read_physical_to_chunk_bytes,
    storage::{read_chunk_verified, ContainerRef},
    types::{ChunkMetadata, ManifestJson, ToolInfo},
};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tonic::{Code, Request, Response as GrpcResponse, Status};
use tower_http::trace::TraceLayer;

use offf_access_service::grpc;
use grpc::offf_access_service_server::{OfffAccessService, OfffAccessServiceServer};
use grpc::{
    AppendAnalysisCorrectionRequest, AppendAnalysisCorrectionResponse,
    AppendProvenanceEventRequest, AppendProvenanceEventResponse, FileRow, GetChunkRequest,
    GetChunkResponse, GetFileRequest, GetFileResponse, GetManifestRequest, GetManifestResponse,
    ListArtifactsRequest, ListArtifactsResponse, ListFilesRequest, ListFilesResponse,
    VerifyChunkRequest, VerifyChunkResponse as GrpcVerifyChunkResponse,
    WriteAnalysisResultsRequest, WriteAnalysisResultsResponse,
};

#[derive(Clone)]
struct AppState {
    cases_root: String,
    tool_registry_path: String,
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
        matches!(self, Self::AnalysisWorker | Self::Indexer | Self::AcquisitionProducer)
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
}

#[derive(Debug)]
enum AuthError {
    Unauthorized(String),
}

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cases_root = env::var("OFFF_CASES_ROOT").unwrap_or_else(|_| "tests/samples".to_string());
    let rest_bind = env::var("OFFF_ACCESS_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let grpc_bind =
        env::var("OFFF_ACCESS_GRPC_BIND").unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    let tool_registry_path = env::var("OFFF_TOOL_REGISTRY")
        .unwrap_or_else(|_| "config/tool-registry.json".to_string());
    let rest_addr: SocketAddr = rest_bind.parse().context("invalid OFFF_ACCESS_BIND")?;
    let grpc_addr: SocketAddr = grpc_bind
        .parse()
        .context("invalid OFFF_ACCESS_GRPC_BIND")?;

    let state = AppState {
        cases_root,
        tool_registry_path,
    };

    let app = Router::new()
        .route("/cases/{case_id}/manifest", get(get_manifest))
        .route("/cases/{case_id}/chunks/{chunk_id}", get(get_chunk))
        .route("/cases/{case_id}/chunks/{chunk_id}/verify", get(verify_chunk_endpoint))
        .route("/cases/{case_id}/files", get(list_files))
        .route("/cases/{case_id}/files/{file_id}", get(get_file))
        .route("/cases/{case_id}/artifacts", get(list_artifacts))
        .route("/cases/{case_id}/analysis-results", post(write_analysis_results))
        .route("/cases/{case_id}/analysis-corrections", post(append_analysis_correction))
        .route("/cases/{case_id}/provenance-events", post(append_provenance_event))
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
        let (case_ref, _, chunks) = load_case_data(&self.state, &req.case_id).map_err(grpc_status)?;
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
        let (case_ref, _, chunks) = load_case_data(&self.state, &req.case_id).map_err(grpc_status)?;
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
        let actor = actor_from_metadata(request.metadata())
            .map_err(grpc_status_from_auth)?;
        if !actor.role.can_write_analysis() {
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
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis)
            .map_err(grpc_status)?;

        let req = request.into_inner();
        let case_ref = resolve_case_ref(&self.state, &req.case_id).map_err(grpc_status)?;

        let mut rows = Vec::with_capacity(req.rows.len());
        for row in req.rows {
            let value: Value = serde_json::from_str(&row.json)
                .map_err(|e| Status::new(Code::InvalidArgument, e.to_string()))?;
            rows.push(value);
        }

        let target = write_analysis_rows(&case_ref, &req.relative_path, &rows).map_err(grpc_status)?;
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
        let actor = actor_from_metadata(request.metadata())
            .map_err(grpc_status_from_auth)?;
        if !actor.role.can_append_provenance() {
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
        enforce_tool_registry(&self.state, &actor, WriteLayer::Provenance)
            .map_err(grpc_status)?;

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
        let actor = actor_from_metadata(request.metadata()).map_err(grpc_status_from_auth)?;
        if !actor.role.can_append_analysis_correction() {
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
        enforce_tool_registry(&self.state, &actor, WriteLayer::Analysis).map_err(grpc_status)?;

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
    let actor = actor_from_headers(&headers)?;
    if !actor.role.can_write_analysis() {
        tracing::warn!(
            action = "write_analysis_results",
            case_id = %case_id,
            tool_id = %actor.tool_id,
            role = ?actor.role,
            outcome = "deny",
            reason = "role cannot write analysis"
        );
        return Err(ApiError::forbidden("role not allowed to write analysis layer"));
    }
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis)?;

    let case_ref = resolve_case_ref(&state, &case_id)?;
    let target = write_analysis_rows(&case_ref, &payload.relative_path, &payload.rows)?;

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
    let actor = actor_from_headers(&headers)?;
    if !actor.role.can_append_provenance() {
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
    enforce_tool_registry(&state, &actor, WriteLayer::Provenance)?;

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
    let actor = actor_from_headers(&headers)?;
    if !actor.role.can_append_analysis_correction() {
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
    enforce_tool_registry(&state, &actor, WriteLayer::Analysis)?;

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

fn list_file_index_values(case_ref: &ContainerRef, partition_id: Option<&str>) -> Result<Vec<Value>, ApiError> {
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

fn write_analysis_rows(case_ref: &ContainerRef, relative_path: &str, rows: &[Value]) -> Result<String, ApiError> {
    let rel = normalize_rel_path(relative_path)?;
    if !rel.starts_with("analysis/") {
        return Err(ApiError::bad_request("relative_path must start with analysis/"));
    }
    if !rel.ends_with(".jsonl") {
        return Err(ApiError::bad_request("only .jsonl is currently supported"));
    }

    if rel.starts_with("indexes/") || rel.starts_with("chunks/") || rel == "manifest.json" {
        return Err(ApiError::forbidden("write target outside analysis layer is not allowed"));
    }

    for row in rows {
        case_ref
            .append_jsonl_line(&rel, &serde_json::to_string(row)?)
            .map_err(ApiError::from)?;
    }

    Ok(rel)
}

fn actor_from_headers(headers: &HeaderMap) -> Result<ActorContext, ApiError> {
    let role_raw = headers
        .get("x-offf-role")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-offf-role"))?;
    let tool_id = headers
        .get("x-offf-tool-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-offf-tool-id"))?
        .trim()
        .to_string();
    if tool_id.is_empty() {
        return Err(ApiError::unauthorized("empty x-offf-tool-id"));
    }

    let role = AppRole::parse(role_raw)
        .ok_or_else(|| ApiError::unauthorized("invalid x-offf-role"))?;

    Ok(ActorContext { role, tool_id })
}

fn actor_from_metadata(metadata: &tonic::metadata::MetadataMap) -> Result<ActorContext, AuthError> {
    let role_raw = metadata
        .get("x-offf-role")
        .ok_or_else(|| AuthError::Unauthorized("missing x-offf-role".to_string()))?
        .to_str()
        .map_err(|_| AuthError::Unauthorized("invalid x-offf-role".to_string()))?;
    let tool_id = metadata
        .get("x-offf-tool-id")
        .ok_or_else(|| AuthError::Unauthorized("missing x-offf-tool-id".to_string()))?
        .to_str()
        .map_err(|_| AuthError::Unauthorized("invalid x-offf-tool-id".to_string()))?
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
        .ok_or_else(|| {
            ApiError::forbidden(format!("tool not registered: {}", actor.tool_id))
        })?;

    if !rec.status.eq_ignore_ascii_case("approved") {
        return Err(ApiError::forbidden(format!(
            "tool is not approved: {}",
            actor.tool_id
        )));
    }

    let role_name = role_name(actor.role);
    if !rec.allowed_roles.iter().any(|r| r.eq_ignore_ascii_case(role_name)) {
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
    event.insert("reason".to_string(), Value::String(reason.trim().to_string()));
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
        for candidate in [format!("{root}/{case_id}"), format!("{root}/{case_id}.offf")] {
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
        .read_bytes(&manifest.indexes.physical_to_chunk)
        .map_err(ApiError::from)?;
    let chunks = read_physical_to_chunk_bytes(&map_data).map_err(ApiError::from)?;
    Ok((case_ref, manifest, chunks))
}

fn read_json_file(case_ref: &ContainerRef, rel: &str) -> Result<Value, ApiError> {
    let raw = case_ref.read_text(rel).map_err(ApiError::from)?;
    let value: Value = serde_json::from_str(&raw)?;
    Ok(value)
}

fn find_chunk<'a>(chunks: &'a [ChunkMetadata], chunk_id: &str) -> Result<&'a ChunkMetadata, ApiError> {
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
