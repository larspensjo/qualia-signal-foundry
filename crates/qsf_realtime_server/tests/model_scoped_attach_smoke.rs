use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use qsf_realtime_protocol::{
    OPENAI_REALTIME_WS_BASE_URL, RealtimeToolDefinition,
    build_openai_realtime_conversation_item_create,
    build_openai_realtime_conversation_session_update, parse_realtime_server_event,
    realtime_event_response_status, realtime_event_type,
};
use qsf_realtime_server::{
    DEFAULT_PCM_RATE_HZ, OPENAI_SAFETY_IDENTIFIER_HEADER, RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES,
    SidebandAttachment, format_connect_error, hash_session_id, state::BrowserSessionConfig,
};
use serde::Serialize;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WebSocketError, Message, client::IntoClientRequest, http::header},
};

const ARTIFACT_PATH_ENV: &str = "QSF_MODEL_SCOPED_ATTACH_ARTIFACT_PATH";
const RESPONSE_TIMEOUT_ENV: &str = "QSF_MODEL_SCOPED_ATTACH_RESPONSE_TIMEOUT_SECS";
const IDLE_CLOSE_TIMEOUT_ENV: &str = "QSF_MODEL_SCOPED_ATTACH_IDLE_CLOSE_TIMEOUT_SECS";
const DEFAULT_ARTIFACT_FILE: &str = "model-scoped-attach-event-shape-inventory.json";
const DEFAULT_IDLE_ARTIFACT_FILE: &str = "model-scoped-attach-idle-close-inventory.json";
const DEFAULT_RESPONSE_TIMEOUT_SECS: u64 = 120;
const DEFAULT_IDLE_CLOSE_TIMEOUT_SECS: u64 = 900;
const LONG_STRING_THRESHOLD_BYTES: usize = 128;
const SMOKE_SESSION_ID: &str = "model-scoped-attach-smoke";
const SMOKE_PROMPT: &str = "Please say one brief greeting without using tools.";
type ProviderWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Serialize)]
struct EventShapeInventory {
    schema_version: u32,
    handshake_accepted: bool,
    advertised_tool_names: BTreeSet<String>,
    events: BTreeMap<String, EventShape>,
    response_statuses: BTreeSet<String>,
    provider_errors: BTreeSet<ProviderErrorIdentifier>,
    #[serde(skip)]
    response_created_at: Option<Instant>,
    response_created_seen: bool,
    first_event_after_response_created: Option<String>,
    output_audio_delta: OutputAudioDeltaObservation,
    session_updated: SessionUpdatedObservation,
    response_done: ResponseDoneObservation,
    idle_close: Option<IdleCloseObservation>,
}

#[derive(Debug, Serialize)]
struct EventShape {
    count: usize,
    first_seen_offset_ms: Option<u64>,
    top_level_keys: BTreeSet<String>,
    field_path_types: BTreeSet<FieldPathType>,
    long_string_byte_lengths: BTreeMap<String, BTreeSet<usize>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct FieldPathType {
    path: String,
    json_type: JsonType,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
enum JsonType {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

#[derive(Debug, Eq, PartialEq)]
struct EventShapeSummary {
    event_type: String,
    top_level_keys: BTreeSet<String>,
    field_path_types: BTreeSet<FieldPathType>,
    long_string_byte_lengths: BTreeMap<String, BTreeSet<usize>>,
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct ProviderErrorIdentifier {
    error_type: Option<String>,
    code: Option<String>,
}

#[derive(Debug)]
struct ProviderErrorObservation {
    identifier: ProviderErrorIdentifier,
    message: Option<String>,
}

#[derive(Debug)]
struct ProviderEventObservation {
    event_type: String,
    response_status: Option<String>,
    provider_error: Option<ProviderErrorObservation>,
}

#[derive(Debug, Default, Serialize)]
struct OutputAudioDeltaObservation {
    event_count: usize,
    event_counts_by_type: BTreeMap<String, usize>,
    delta_string_count: usize,
    delta_string_base64_decodable_count: usize,
    delta_string_not_base64_decodable_count: usize,
}

#[derive(Debug, Default, Serialize)]
struct SessionUpdatedObservation {
    session_keys: BTreeSet<String>,
    advertised_tool_count: Option<usize>,
}

#[derive(Debug, Default, Serialize)]
struct ResponseDoneObservation {
    response_keys: BTreeSet<String>,
    usage_keys: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
struct IdleCloseObservation {
    outcome: String,
    offset_ms_from_session_updated: u64,
    cap_ms: u64,
    error_category: Option<String>,
}

impl Default for EventShapeInventory {
    fn default() -> Self {
        Self {
            schema_version: 2,
            handshake_accepted: false,
            advertised_tool_names: BTreeSet::new(),
            events: BTreeMap::new(),
            response_statuses: BTreeSet::new(),
            provider_errors: BTreeSet::new(),
            response_created_at: None,
            response_created_seen: false,
            first_event_after_response_created: None,
            output_audio_delta: OutputAudioDeltaObservation::default(),
            session_updated: SessionUpdatedObservation::default(),
            response_done: ResponseDoneObservation::default(),
            idle_close: None,
        }
    }
}

impl EventShapeInventory {
    fn record_advertised_tools(&mut self, tools: &[RealtimeToolDefinition]) {
        self.advertised_tool_names
            .extend(tools.iter().map(|tool| tool.name.clone()));
    }

    fn observe_event(&mut self, event: &Value, observed_at: Instant) {
        let Some(summary) = summarize_event_shape(event) else {
            return;
        };

        if summary.event_type == "response.created" && !self.response_created_seen {
            self.response_created_at = Some(observed_at);
            self.response_created_seen = true;
        }

        let offset_ms = if summary.event_type == "response.created" {
            Some(0)
        } else {
            self.response_created_at
                .map(|created_at| elapsed_ms(created_at, observed_at))
        };

        if summary.event_type == "response.created" {
            self.record_shape(summary, offset_ms);
        } else {
            if self.response_created_seen && self.first_event_after_response_created.is_none() {
                self.first_event_after_response_created = Some(summary.event_type.clone());
            }
            self.record_shape(summary, offset_ms);
        }

        if let Some(status) = realtime_event_response_status(event) {
            self.response_statuses.insert(status.to_string());
        }
        if let Some(provider_error) = provider_error_observation(event) {
            self.provider_errors.insert(provider_error.identifier);
        }

        match realtime_event_type(event) {
            Some(event_type) if RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES.contains(&event_type) => {
                self.observe_output_audio_delta(event_type, event);
            }
            Some("session.updated") => {
                self.observe_session_updated(event);
            }
            Some("response.done") => {
                self.observe_response_done(event);
            }
            _ => {}
        }
    }

    fn record_shape(&mut self, summary: EventShapeSummary, offset_ms: Option<u64>) {
        let shape = self
            .events
            .entry(summary.event_type)
            .or_insert_with(|| EventShape {
                count: 0,
                first_seen_offset_ms: offset_ms,
                top_level_keys: BTreeSet::new(),
                field_path_types: BTreeSet::new(),
                long_string_byte_lengths: BTreeMap::new(),
            });
        shape.count += 1;
        shape.top_level_keys.extend(summary.top_level_keys);
        shape.field_path_types.extend(summary.field_path_types);
        for (key, lengths) in summary.long_string_byte_lengths {
            shape
                .long_string_byte_lengths
                .entry(key)
                .or_default()
                .extend(lengths);
        }
    }

    fn observe_output_audio_delta(&mut self, event_type: &str, event: &Value) {
        self.output_audio_delta.event_count += 1;
        *self
            .output_audio_delta
            .event_counts_by_type
            .entry(event_type.to_string())
            .or_default() += 1;
        let Some(delta) = event.get("delta").and_then(Value::as_str) else {
            return;
        };
        self.output_audio_delta.delta_string_count += 1;
        if base64::engine::general_purpose::STANDARD
            .decode(delta)
            .is_ok()
        {
            self.output_audio_delta.delta_string_base64_decodable_count += 1;
        } else {
            self.output_audio_delta
                .delta_string_not_base64_decodable_count += 1;
        }
    }

    fn observe_session_updated(&mut self, event: &Value) {
        let Some(session) = event.get("session").and_then(Value::as_object) else {
            return;
        };
        self.session_updated
            .session_keys
            .extend(session.keys().cloned());
        self.session_updated.advertised_tool_count =
            session.get("tools").and_then(Value::as_array).map(Vec::len);
    }

    fn observe_response_done(&mut self, event: &Value) {
        let Some(response) = event.get("response").and_then(Value::as_object) else {
            return;
        };
        self.response_done
            .response_keys
            .extend(response.keys().cloned());
        if let Some(usage) = response.get("usage").and_then(Value::as_object) {
            self.response_done.usage_keys.extend(usage.keys().cloned());
        }
    }
}

/// Summarize one parsed provider event without retaining any payload value.
///
/// This is deliberately a pure function over JSON. The live reader supplies only
/// its result to the artifact accumulator, so payload text and audio cannot cross
/// the artifact boundary accidentally.
fn summarize_event_shape(event: &Value) -> Option<EventShapeSummary> {
    let object = event.as_object()?;
    let event_type = object.get("type")?.as_str()?.to_string();
    let top_level_keys = object.keys().cloned().collect();
    let mut field_path_types = BTreeSet::new();
    for (key, value) in object {
        collect_field_path_types(value, key, &mut field_path_types);
    }
    let mut long_string_byte_lengths = BTreeMap::new();
    collect_long_string_lengths(event, "", &mut long_string_byte_lengths);

    Some(EventShapeSummary {
        event_type,
        top_level_keys,
        field_path_types,
        long_string_byte_lengths,
    })
}

fn collect_field_path_types(
    value: &Value,
    path: &str,
    field_path_types: &mut BTreeSet<FieldPathType>,
) {
    field_path_types.insert(FieldPathType {
        path: path.to_string(),
        json_type: json_type(value),
    });
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                collect_field_path_types(child, &format!("{path}.{key}"), field_path_types);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_field_path_types(child, &format!("{path}[]"), field_path_types);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn json_type(value: &Value) -> JsonType {
    match value {
        Value::Null => JsonType::Null,
        Value::Bool(_) => JsonType::Boolean,
        Value::Number(_) => JsonType::Number,
        Value::String(_) => JsonType::String,
        Value::Array(_) => JsonType::Array,
        Value::Object(_) => JsonType::Object,
    }
}

fn collect_long_string_lengths(
    value: &Value,
    path: &str,
    lengths: &mut BTreeMap<String, BTreeSet<usize>>,
) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_path = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                collect_long_string_lengths(child, &child_path, lengths);
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                collect_long_string_lengths(child, &format!("{path}[{index}]"), lengths);
            }
        }
        Value::String(string) if string.len() >= LONG_STRING_THRESHOLD_BYTES => {
            lengths
                .entry(path.to_string())
                .or_default()
                .insert(string.len());
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn provider_error_observation(event: &Value) -> Option<ProviderErrorObservation> {
    if realtime_event_type(event) != Some("error") {
        return None;
    }
    let error = event.get("error").and_then(Value::as_object);
    Some(ProviderErrorObservation {
        identifier: ProviderErrorIdentifier {
            error_type: error
                .and_then(|error| error.get("type"))
                .and_then(Value::as_str)
                .map(str::to_string),
            code: error
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        message: error
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn elapsed_ms(start: Instant, end: Instant) -> u64 {
    end.saturating_duration_since(start).as_millis() as u64
}

fn default_artifact_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../state")
        .join(file_name)
}

fn smoke_artifact_path() -> PathBuf {
    env::var_os(ARTIFACT_PATH_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_artifact_path(DEFAULT_ARTIFACT_FILE))
}

fn idle_artifact_path() -> PathBuf {
    default_artifact_path(DEFAULT_IDLE_ARTIFACT_FILE)
}

fn configured_duration(env_name: &str, default_seconds: u64) -> Result<Duration> {
    let seconds = env::var(env_name)
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()
        .with_context(|| format!("{env_name} must be an integer number of seconds"))?
        .unwrap_or(default_seconds);
    if seconds == 0 {
        bail!("{env_name} must be greater than zero");
    }
    Ok(Duration::from_secs(seconds))
}

fn write_event_shape_inventory(path: &Path, inventory: &EventShapeInventory) -> Result<PathBuf> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create artifact directory `{}`", parent.display())
        })?;
    }
    let file = std::fs::File::create(path)
        .with_context(|| format!("failed to create artifact `{}`", path.display()))?;
    serde_json::to_writer_pretty(file, inventory)
        .with_context(|| format!("failed to write artifact `{}`", path.display()))?;
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize artifact path `{}`", path.display()))
}

enum IncomingMessage {
    Event(ProviderEventObservation),
    Closed,
    Ignored,
}

async fn read_and_record(
    websocket: &mut ProviderWebSocket,
    inventory: &mut EventShapeInventory,
) -> std::result::Result<IncomingMessage, WebSocketError> {
    let Some(message) = websocket.next().await else {
        return Ok(IncomingMessage::Closed);
    };
    let message = message?;
    match message {
        Message::Text(text) => {
            let Some(event) = parse_realtime_server_event("model_scoped_attach_smoke", &text)
            else {
                return Ok(IncomingMessage::Ignored);
            };
            let Some(event_type) = realtime_event_type(&event).map(str::to_owned) else {
                return Ok(IncomingMessage::Ignored);
            };
            let response_status = realtime_event_response_status(&event).map(str::to_owned);
            let provider_error = provider_error_observation(&event);
            inventory.observe_event(&event, Instant::now());
            Ok(IncomingMessage::Event(ProviderEventObservation {
                event_type,
                response_status,
                provider_error,
            }))
        }
        Message::Ping(payload) => {
            websocket.send(Message::Pong(payload)).await?;
            Ok(IncomingMessage::Ignored)
        }
        Message::Close(_) => Ok(IncomingMessage::Closed),
        Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => Ok(IncomingMessage::Ignored),
    }
}

async fn read_before_deadline(
    websocket: &mut ProviderWebSocket,
    inventory: &mut EventShapeInventory,
    deadline: tokio::time::Instant,
) -> Result<IncomingMessage> {
    tokio::time::timeout_at(deadline, read_and_record(websocket, inventory))
        .await
        .context("provider websocket read timed out")?
        .context("failed to read provider websocket message")
}

fn fail_on_provider_error(event: &ProviderEventObservation, expected_event: &str) -> Result<()> {
    let Some(error) = &event.provider_error else {
        return Ok(());
    };
    let error_type = error
        .identifier
        .error_type
        .as_deref()
        .unwrap_or("<missing>");
    let code = error.identifier.code.as_deref().unwrap_or("<missing>");
    println!("provider error event: type={error_type}; code={code}");
    let message = error.message.as_deref().unwrap_or("<no message>");
    bail!(
        "provider error before {expected_event}: type={error_type}; code={code}; message={message}"
    )
}

async fn connect_model_scoped(
    config: &BrowserSessionConfig,
    inventory: &mut EventShapeInventory,
) -> Result<ProviderWebSocket> {
    inventory.record_advertised_tools(&config.tools);
    let api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;
    let attachment = SidebandAttachment::ServerModelSession {
        model: config.model.clone(),
    };
    let websocket_url = attachment.websocket_url(OPENAI_REALTIME_WS_BASE_URL);
    let mut request = websocket_url
        .into_client_request()
        .context("failed to build model-scoped websocket request")?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {api_key}")
            .parse()
            .context("failed to build Authorization header")?,
    );
    request.headers_mut().insert(
        OPENAI_SAFETY_IDENTIFIER_HEADER,
        hash_session_id(SMOKE_SESSION_ID)
            .parse()
            .context("failed to build OpenAI-Safety-Identifier header")?,
    );

    let (websocket, _) = connect_async(request)
        .await
        .map_err(|error| anyhow::anyhow!(format_connect_error(SMOKE_SESSION_ID, &error)))?;
    inventory.handshake_accepted = true;
    println!("model-scoped websocket handshake accepted");
    Ok(websocket)
}

async fn send_production_session_update(
    websocket: &mut ProviderWebSocket,
    config: &BrowserSessionConfig,
) -> Result<()> {
    let session_update = build_openai_realtime_conversation_session_update(
        &config.model,
        &config.voice,
        &config.instructions,
        &config.output_modalities,
        DEFAULT_PCM_RATE_HZ,
        false,
        false,
        &config.tools,
        Some("auto"),
        config.input_transcription_model.as_deref(),
    );
    websocket
        .send(Message::Text(session_update.to_string().into()))
        .await
        .context("failed to send session.update")
}

async fn wait_for_session_updated(
    websocket: &mut ProviderWebSocket,
    inventory: &mut EventShapeInventory,
    response_timeout: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + response_timeout;
    loop {
        match read_before_deadline(websocket, inventory, deadline).await? {
            IncomingMessage::Event(event) => {
                fail_on_provider_error(&event, "session.updated")?;
                if event.event_type == "session.updated" {
                    return Ok(());
                }
            }
            IncomingMessage::Closed => bail!("provider closed before session.updated"),
            IncomingMessage::Ignored => {}
        }
    }
}

async fn capture_billed_turn(
    inventory: &mut EventShapeInventory,
    response_timeout: Duration,
) -> Result<()> {
    let config = BrowserSessionConfig::default();
    let mut websocket = connect_model_scoped(&config, inventory).await?;
    send_production_session_update(&mut websocket, &config).await?;
    wait_for_session_updated(&mut websocket, inventory, response_timeout).await?;
    println!("session.updated observed; sending one smoke turn");

    let item = build_openai_realtime_conversation_item_create("user", SMOKE_PROMPT);
    websocket
        .send(Message::Text(item.to_string().into()))
        .await
        .context("failed to send conversation.item.create")?;
    let response = qsf_realtime_protocol::build_openai_realtime_response_create(
        &config.voice,
        &config.instructions,
        DEFAULT_PCM_RATE_HZ,
    );
    websocket
        .send(Message::Text(response.to_string().into()))
        .await
        .context("failed to send response.create")?;

    let response_deadline = tokio::time::Instant::now() + response_timeout;
    loop {
        match read_before_deadline(&mut websocket, inventory, response_deadline).await? {
            IncomingMessage::Event(event) => {
                fail_on_provider_error(&event, "response.done")?;
                if event.event_type == "response.done" {
                    let status = event.response_status.as_deref().unwrap_or("<missing>");
                    println!("response.done observed with response.status={status}");
                    if status != "completed" {
                        bail!("response.done reported non-completed response.status={status}");
                    }
                    return Ok(());
                }
            }
            IncomingMessage::Closed => bail!("provider closed before response.done"),
            IncomingMessage::Ignored => {}
        }
    }
}

async fn capture_idle_close(
    inventory: &mut EventShapeInventory,
    response_timeout: Duration,
    idle_close_timeout: Duration,
) -> Result<()> {
    let config = BrowserSessionConfig::default();
    let mut websocket = connect_model_scoped(&config, inventory).await?;
    send_production_session_update(&mut websocket, &config).await?;
    wait_for_session_updated(&mut websocket, inventory, response_timeout).await?;
    println!("session.updated observed; submitting no turn and waiting for provider close");

    let idle_started_at = Instant::now();
    let cap_ms = idle_close_timeout.as_millis() as u64;
    let idle_deadline = tokio::time::Instant::now() + idle_close_timeout;
    loop {
        match tokio::time::timeout_at(idle_deadline, read_and_record(&mut websocket, inventory))
            .await
        {
            Ok(Ok(IncomingMessage::Closed)) => {
                inventory.idle_close = Some(IdleCloseObservation {
                    outcome: "provider_closed".to_string(),
                    offset_ms_from_session_updated: elapsed_ms(idle_started_at, Instant::now()),
                    cap_ms,
                    error_category: None,
                });
                break;
            }
            Ok(Ok(IncomingMessage::Event(event))) => {
                if let Some(error) = event.provider_error {
                    let error_type = error
                        .identifier
                        .error_type
                        .as_deref()
                        .unwrap_or("<missing>");
                    let code = error.identifier.code.as_deref().unwrap_or("<missing>");
                    println!("provider error event while idle: type={error_type}; code={code}");
                }
            }
            Ok(Ok(IncomingMessage::Ignored)) => {}
            Ok(Err(error)) => {
                inventory.idle_close = Some(IdleCloseObservation {
                    outcome: "provider_closed_with_error".to_string(),
                    offset_ms_from_session_updated: elapsed_ms(idle_started_at, Instant::now()),
                    cap_ms,
                    error_category: Some(websocket_error_category(&error).to_string()),
                });
                break;
            }
            Err(_) => {
                println!(
                    "observation 5 unanswered; raise \
                     QSF_MODEL_SCOPED_ATTACH_IDLE_CLOSE_TIMEOUT_SECS"
                );
                inventory.idle_close = Some(IdleCloseObservation {
                    outcome: "cap_elapsed".to_string(),
                    offset_ms_from_session_updated: elapsed_ms(idle_started_at, Instant::now()),
                    cap_ms,
                    error_category: None,
                });
                let _ = websocket.send(Message::Close(None)).await;
                break;
            }
        }
    }
    Ok(())
}

fn websocket_error_category(error: &WebSocketError) -> &'static str {
    match error {
        WebSocketError::ConnectionClosed => "connection_closed",
        WebSocketError::AlreadyClosed => "already_closed",
        WebSocketError::Io(_) => "io",
        WebSocketError::Tls(_) => "tls",
        WebSocketError::Capacity(_) => "capacity",
        WebSocketError::Protocol(_) => "protocol",
        WebSocketError::WriteBufferFull(_) => "write_buffer_full",
        WebSocketError::Utf8(_) => "utf8",
        WebSocketError::AttackAttempt => "attack_attempt",
        WebSocketError::Url(_) => "url",
        WebSocketError::Http(_) => "http",
        WebSocketError::HttpFormat(_) => "http_format",
    }
}

fn print_capture_error(capture_result: &Result<()>) {
    if let Err(error) = capture_result {
        eprintln!("live capture failed before artifact write: {error:#}");
    }
}

#[tokio::test]
#[ignore = "requires a paid live OpenAI Realtime session"]
async fn model_scoped_attach_smoke() {
    let path = smoke_artifact_path();
    let response_timeout = configured_duration(RESPONSE_TIMEOUT_ENV, DEFAULT_RESPONSE_TIMEOUT_SECS)
        .expect("valid response timeout configuration");
    let mut inventory = EventShapeInventory::default();
    let capture_result = capture_billed_turn(&mut inventory, response_timeout).await;
    print_capture_error(&capture_result);
    let canonical_path = write_event_shape_inventory(&path, &inventory)
        .expect("event-shape inventory must be written");
    println!(
        "event-shape inventory written to {}",
        canonical_path.display()
    );
    println!(
        "first event after response.created: {:?}; output audio delta event counts: {:?}; delta strings base64-decodable: {}",
        inventory.first_event_after_response_created,
        inventory.output_audio_delta.event_counts_by_type,
        inventory
            .output_audio_delta
            .delta_string_base64_decodable_count
    );
    capture_result.expect("model-scoped attach smoke run failed");
}

#[tokio::test]
#[ignore = "requires a live OpenAI Realtime session; submits no turn and bills no tokens"]
async fn model_scoped_attach_idle_close_probe() {
    let path = idle_artifact_path();
    let response_timeout = configured_duration(RESPONSE_TIMEOUT_ENV, DEFAULT_RESPONSE_TIMEOUT_SECS)
        .expect("valid response timeout configuration");
    let idle_close_timeout =
        configured_duration(IDLE_CLOSE_TIMEOUT_ENV, DEFAULT_IDLE_CLOSE_TIMEOUT_SECS)
            .expect("valid idle close timeout configuration");
    let mut inventory = EventShapeInventory::default();
    let capture_result =
        capture_idle_close(&mut inventory, response_timeout, idle_close_timeout).await;
    print_capture_error(&capture_result);
    let canonical_path = write_event_shape_inventory(&path, &inventory)
        .expect("idle-close inventory must be written");
    println!(
        "idle-close inventory written to {}",
        canonical_path.display()
    );
    println!("idle-close observation: {:?}", inventory.idle_close);
    capture_result.expect("model-scoped attach idle-close probe failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_shape_summary_keeps_paths_types_and_lengths_but_never_payload_text() {
        let payload = "synthetic-audio-payload-that-must-not-escape-".repeat(8);
        let event = serde_json::json!({
            "type": "response.output_audio.delta",
            "delta": payload,
            "event_id": "evt-1",
            "response": {
                "output": [{
                    "type": "message",
                    "content": [{
                        "type": "audio",
                        "transcript": "short-secret"
                    }]
                }]
            }
        });

        let summary = summarize_event_shape(&event).expect("event shape");
        assert_eq!(summary.event_type, "response.output_audio.delta");
        assert_eq!(
            summary.long_string_byte_lengths["delta"],
            BTreeSet::from([payload.len()])
        );
        assert!(summary.field_path_types.contains(&FieldPathType {
            path: "response.output[].content[].transcript".to_string(),
            json_type: JsonType::String,
        }));
        assert!(summary.field_path_types.contains(&FieldPathType {
            path: "response.output[]".to_string(),
            json_type: JsonType::Object,
        }));
        let serialized = serde_json::to_string(&summary.field_path_types).unwrap();
        assert!(!serialized.contains("synthetic-audio-payload"));
        assert!(!serialized.contains("short-secret"));
        assert!(!serialized.contains("evt-1"));
    }

    #[test]
    fn inventory_records_nested_usage_status_errors_and_only_approved_string_values() {
        let mut inventory = EventShapeInventory::default();
        inventory.record_advertised_tools(&[RealtimeToolDefinition::function(
            "approved_tool_name",
            "description-must-not-be-recorded",
            serde_json::json!({"type":"object","properties":{"secret":{"type":"string"}}}),
        )]);
        let start = Instant::now();
        inventory.observe_event(
            &serde_json::json!({
                "type":"session.updated",
                "session":{"tools":[{"name":"provider-tool-value"}],"model":"hidden"}
            }),
            start,
        );
        inventory.observe_event(
            &serde_json::json!({
                "type":"response.created",
                "response":{"id":"hidden","status":"in_progress"}
            }),
            start + Duration::from_millis(5),
        );
        inventory.observe_event(
            &serde_json::json!({
                "type":"error",
                "error":{
                    "type":"invalid_request_error",
                    "code":"invalid_value",
                    "message":"provider-message-must-not-be-recorded"
                }
            }),
            start + Duration::from_millis(6),
        );
        inventory.observe_event(
            &serde_json::json!({
                "type":"response.done",
                "response":{
                    "status":"completed",
                    "output":[{
                        "type":"message",
                        "content":[{"type":"audio","transcript":"hidden-transcript"}]
                    }],
                    "usage":{
                        "input_token_details":{
                            "text_tokens":7,
                            "cached_tokens_details":{"text_tokens":3}
                        },
                        "output_token_details":{"audio_tokens":11}
                    }
                }
            }),
            start + Duration::from_millis(12),
        );

        assert_eq!(
            inventory.first_event_after_response_created.as_deref(),
            Some("error")
        );
        assert_eq!(
            inventory.events["response.done"].first_seen_offset_ms,
            Some(7)
        );
        assert!(
            inventory.events["response.done"]
                .field_path_types
                .contains(&FieldPathType {
                    path: "response.usage.input_token_details.cached_tokens_details.text_tokens"
                        .to_string(),
                    json_type: JsonType::Number,
                })
        );
        assert_eq!(
            inventory.response_statuses,
            BTreeSet::from(["completed".to_string(), "in_progress".to_string()])
        );
        assert_eq!(
            inventory.provider_errors,
            BTreeSet::from([ProviderErrorIdentifier {
                error_type: Some("invalid_request_error".to_string()),
                code: Some("invalid_value".to_string()),
            }])
        );
        assert_eq!(
            inventory.advertised_tool_names,
            BTreeSet::from(["approved_tool_name".to_string()])
        );
        let serialized = serde_json::to_string(&inventory).unwrap();
        for forbidden in [
            "hidden",
            "hidden-transcript",
            "provider-tool-value",
            "provider-message-must-not-be-recorded",
            "description-must-not-be-recorded",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "artifact leaked forbidden string value `{forbidden}`"
            );
        }
    }

    #[test]
    fn both_output_audio_delta_event_names_use_the_base64_probe() {
        let mut inventory = EventShapeInventory::default();
        let encoded = base64::engine::general_purpose::STANDARD.encode([1_u8, 2, 3]);
        inventory.observe_event(
            &serde_json::json!({"type":"response.output_audio.delta","delta":encoded}),
            Instant::now(),
        );
        inventory.observe_event(
            &serde_json::json!({"type":"response.audio.delta","delta":"not base64"}),
            Instant::now(),
        );

        assert_eq!(inventory.output_audio_delta.event_count, 2);
        assert_eq!(
            inventory.output_audio_delta.event_counts_by_type,
            BTreeMap::from([
                ("response.audio.delta".to_string(), 1),
                ("response.output_audio.delta".to_string(), 1),
            ])
        );
        assert_eq!(
            inventory
                .output_audio_delta
                .delta_string_base64_decodable_count,
            1
        );
        assert_eq!(
            inventory
                .output_audio_delta
                .delta_string_not_base64_decodable_count,
            1
        );
    }
}
