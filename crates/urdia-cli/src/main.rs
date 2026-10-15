//! Urdia's small, local-first command line interface.

mod agent_store;
mod control_probe;
mod credential_store;
mod local_jev;
mod local_laya;
mod profile_evaluation;
mod profile_store;
mod profiles;
mod result_source;
mod routing_eval;
mod verification;

use profiles::{ExecutionProfile, ProfileCatalog};

use async_trait::async_trait;
use clap::{Parser, Subcommand, ValueEnum};
use routing_eval::{EvaluationSplit, evaluate_routing};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::{Future, poll_fn};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use urdia_acp::{AcpCommand, AcpProvider};
use urdia_core::{
    AgentSession, AgentUpdate, AgentUpdateSink, AuthenticationState, Event, EventKind, EventStore,
    FirstCandidateDecisionProvider, InferenceLocation, PermissionReceipt, PermissionReceiptSink,
    Prompt, ProviderAvailability, ProviderInfo, ProviderRegistry, Result, SessionRequest,
    UrdiaError, WorkspacePath,
};
use urdia_decision_jev::JevDecisionProvider;
use urdia_decision_laya::LayaDecisionProvider;
use urdia_decision_typellm::TypeLlmDecisionProvider;
use urdia_provider_qwen::QwenProvider;
use urdia_routing::{RouteCandidate, RoutePolicy, RouteRequest, RouteResolver};
use urdia_store_sqlite::SqliteEventStore;
use urdia_ui::{
    ConversationCommand, ConversationContext, ConversationHub, ConversationProfile,
    ConversationTurnStatus, ConversationUpdateError, InteractivePermissionBroker, LiveUiServer,
    load_timeline, render_timeline,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "urdia",
    version,
    about = "An open, provider-neutral agent workspace"
)]
struct Cli {
    /// Optional TOML file containing ACP launch definitions and execution profiles.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Explicit profile-only settings file; enables dashboard saves for the next launch.
    #[arg(long, global = true, value_name = "PATH")]
    profile_store: Option<PathBuf>,
    /// Private launch-only settings file; enables next-launch agent registration.
    #[arg(long, global = true, value_name = "PATH")]
    agent_store: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect local capabilities without starting an agent or reading provider credentials.
    Doctor,
    /// List registered agent providers and conservative runtime metadata.
    Providers,
    /// List configured execution profiles without starting agents or inspecting keys.
    Profiles {
        #[arg(long)]
        json: bool,
    },
    /// Recommend one available provider without starting it or granting permissions.
    Route {
        /// Task that needs a provider recommendation.
        #[arg(long)]
        objective: String,
        /// Decision engine used for this advisory recommendation.
        #[arg(long, value_enum, default_value_t = RouterKind::Deterministic)]
        router: RouterKind,
        /// Available provider ID to consider; repeat to control the candidate set.
        #[arg(long = "candidate")]
        candidates: Vec<String>,
        /// Optional bounded JSON context sent to the decision engine.
        #[arg(long)]
        state: Option<PathBuf>,
        /// Optional post-decision confidence threshold.
        #[arg(long)]
        min_confidence: Option<f32>,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run the versioned routing corpus without starting an agent.
    EvalRouting {
        /// Evaluation corpus JSON file.
        #[arg(long, default_value = "evaluations/routing/v2.json")]
        fixture: PathBuf,
        /// Decision engine to evaluate.
        #[arg(long, value_enum, default_value_t = RouterKind::Deterministic)]
        router: RouterKind,
        /// Required acknowledgement before sending fixture data to remote Jev.
        #[arg(long, default_value_t = false)]
        live: bool,
        /// Optional post-decision confidence threshold.
        #[arg(long)]
        min_confidence: Option<f32>,
        /// Fixture partition to evaluate.
        #[arg(long, value_enum, default_value_t = EvaluationSplit::All)]
        split: EvaluationSplit,
        /// Number of sequential runs per case; live Jev repeats consume API quota.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
        repeats: u8,
    },
    /// Generate a private, read-only HTML timeline from an existing event database.
    Timeline {
        /// Existing SQLite event database path.
        #[arg(long, default_value = ".urdia/events.sqlite")]
        database: PathBuf,
        /// Standalone HTML file to create or replace.
        #[arg(long, default_value = ".urdia/timeline.html")]
        output: PathBuf,
        /// Maximum number of recent sessions to include.
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=500))]
        limit: u16,
    },
    /// Serve a private, live, read-only timeline on a loopback address.
    Ui {
        /// Existing SQLite event database path.
        #[arg(long, default_value = ".urdia/events.sqlite")]
        database: PathBuf,
        /// Loopback socket address; port zero selects a free ephemeral port.
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: SocketAddr,
        /// Maximum number of recent sessions to include.
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=500))]
        limit: u16,
    },
    /// Run one explicit provider session in a workspace.
    Run {
        /// Stable provider ID shown by `urdia providers`.
        #[arg(long, conflicts_with = "profile")]
        provider: Option<String>,
        /// Explicit configured launch profile, overriding default_profile.
        #[arg(long, conflicts_with = "provider")]
        profile: Option<String>,
        /// Existing workspace directory.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Prompt sent after provider session setup.
        #[arg(long)]
        prompt: String,
        /// SQLite event database path.
        #[arg(long, default_value = ".urdia/events.sqlite")]
        database: PathBuf,
        /// Persist prompt and response text in the local event database.
        #[arg(long, default_value_t = false)]
        persist_content: bool,
        /// Resolve one-shot agent permissions from a private local browser UI.
        #[arg(long, default_value_t = false)]
        permissions_ui: bool,
        /// Opt into a separately confirmed, 15-minute trusted permission session.
        #[arg(long, requires = "permissions_ui", default_value_t = false)]
        trusted_session: bool,
        /// Loopback socket for the permission UI; port zero selects a free port.
        #[arg(long, requires = "permissions_ui")]
        ui_bind: Option<SocketAddr>,
    },
    /// Start a private browser conversation with one explicit provider.
    Chat {
        /// Stable provider ID shown by `urdia providers`.
        #[arg(long, conflicts_with = "profile")]
        provider: Option<String>,
        /// Explicit configured launch profile, overriding default_profile.
        #[arg(long, conflicts_with = "provider")]
        profile: Option<String>,
        /// Existing workspace directory.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// SQLite event database path.
        #[arg(long, default_value = ".urdia/events.sqlite")]
        database: PathBuf,
        /// Persist prompt and response text in the local event database.
        #[arg(long, default_value_t = false)]
        persist_content: bool,
        /// Resolve one-shot agent permissions from the same private browser UI.
        #[arg(long, default_value_t = false)]
        permissions_ui: bool,
        /// Offer explicitly confirmed project checks (Linux/WSL + Python).
        #[arg(long, default_value_t = false)]
        verification_ui: bool,
        /// Enable explicit browser advice using host-configured decision engines
        #[arg(long, default_value_t = false)]
        advisory_ui: bool,
        /// Recommend configured execution profiles rather than bare agent providers.
        #[arg(long, requires = "advisory_ui", default_value_t = false)]
        advisory_profiles: bool,
        /// Opt into a separately confirmed, 15-minute trusted permission session.
        #[arg(long, requires = "permissions_ui", default_value_t = false)]
        trusted_session: bool,
        /// Loopback socket for the conversation UI; port zero selects a free port.
        #[arg(long, default_value = "127.0.0.1:0")]
        bind: SocketAddr,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum RouterKind {
    Deterministic,
    Jev,
    Laya,
    Typellm,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct UrdiaConfig {
    #[serde(default, rename = "provider")]
    providers: Vec<ExternalAcpProvider>,
    #[serde(default, rename = "profile")]
    profiles: Vec<ExecutionProfile>,
    default_profile: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalAcpProvider {
    id: String,
    name: String,
    command: PathBuf,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Clone, Debug)]
struct SqlitePermissionReceiptSink {
    store: SqliteEventStore,
}

#[async_trait]
impl PermissionReceiptSink for SqlitePermissionReceiptSink {
    async fn record(&self, receipt: PermissionReceipt) -> Result<()> {
        let session_id = receipt.session_id.clone();
        let data = serde_json::to_value(receipt)
            .map_err(|error| UrdiaError::Store(format!("serialize permission receipt: {error}")))?;
        self.store
            .append(Event::new(session_id, EventKind::Permission, data))
            .await
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Command::Timeline {
            database,
            output,
            limit,
        } => {
            return match generate_timeline(database, output, usize::from(*limit)).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => report_error(error),
            };
        }
        Command::Ui {
            database,
            bind,
            limit,
        } => {
            return match serve_live_ui(database, *bind, usize::from(*limit)).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => report_error(error),
            };
        }
        Command::Doctor
        | Command::Providers
        | Command::Profiles { .. }
        | Command::Route { .. }
        | Command::EvalRouting { .. }
        | Command::Run { .. }
        | Command::Chat { .. } => {}
    }
    let mut agent_store = match agent_store::AgentStore::load(cli.agent_store.as_deref()) {
        Ok(store) => store,
        Err(error) => return report_error(error),
    };
    // Each destination has a distinct schema and must never overwrite another.
    for other in [cli.config.as_deref(), cli.profile_store.as_deref()]
        .into_iter()
        .flatten()
    {
        if cli
            .agent_store
            .as_deref()
            .is_some_and(|path| !matches!(distinct_settings(path, other), Ok(true)))
        {
            return report_error(UrdiaError::InvalidInput(
                "agent store must be separate from other configuration".into(),
            ));
        }
    }
    let (registry, profiles) =
        match build_configuration_with_agents(cli.config.as_deref(), agent_store.definitions()) {
            Ok(configuration) => configuration,
            Err(error) => return report_error(error),
        };

    agent_store.bind_registry(&registry);
    let agent_store = Arc::new(agent_store);
    if let (Some(config), Some(store)) = (cli.config.as_deref(), cli.profile_store.as_deref()) {
        match distinct_settings(config, store) {
            Ok(true) => {}
            _ => {
                return report_error(UrdiaError::InvalidInput(
                    "profile store must be separate from provider configuration".into(),
                ));
            }
        }
    }
    let profile_store = match profile_store::ProfileStore::new(
        &registry,
        &profiles,
        cli.profile_store.as_deref(),
    ) {
        Ok(store) => Arc::new(store),
        Err(error) => return report_error(error),
    };
    let profiles = profile_store.active_catalog();
    match cli.command {
        Command::Doctor => {
            doctor(&registry);
            ExitCode::SUCCESS
        }
        Command::Providers => {
            print_providers(&registry);
            ExitCode::SUCCESS
        }
        Command::Profiles { json } => {
            let entries: Vec<Value> = profiles.profiles.values().map(|profile| {
                json!({"profile": profile,
                    "default": profiles.default_profile.as_deref() == Some(profile.id.as_str()),
                    "executable_available": registry.resolve(&profile.provider).expect("validated provider").info().availability.is_available(),
                    "model_effort_verified": false})
            }).collect();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&entries).expect("profile JSON")
                );
            } else {
                for entry in entries {
                    println!("{}", entry);
                }
            }
            ExitCode::SUCCESS
        }
        Command::Route {
            objective,
            router,
            candidates,
            state,
            min_confidence,
            json,
        } => match recommend_route(
            &registry,
            objective,
            router,
            candidates,
            state.as_deref(),
            min_confidence,
            json,
        )
        .await
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(error),
        },
        Command::EvalRouting {
            fixture,
            router,
            live,
            min_confidence,
            split,
            repeats,
        } => match evaluate_routing(&fixture, router, live, min_confidence, split, repeats).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => report_error(error),
        },
        Command::Timeline { .. } => unreachable!("timeline returns before provider discovery"),
        Command::Ui { .. } => unreachable!("UI returns before provider discovery"),
        Command::Run {
            provider,
            profile,
            workspace,
            prompt,
            database,
            persist_content,
            permissions_ui,
            trusted_session,
            ui_bind,
        } => {
            let controls = profiles.controls(provider.as_deref(), profile.as_deref());
            let provider = match profiles.select(provider.as_deref(), profile.as_deref()) {
                Ok(provider) => provider,
                Err(error) => return report_error(error),
            };
            match run_provider(
                &registry,
                &provider,
                controls,
                workspace,
                prompt,
                database,
                persist_content,
                trusted_session,
                permissions_ui.then(|| {
                    ui_bind.unwrap_or_else(|| {
                        "127.0.0.1:0"
                            .parse()
                            .expect("built-in loopback address must parse")
                    })
                }),
            )
            .await
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => report_error(error),
            }
        }
        Command::Chat {
            provider,
            profile,
            workspace,
            database,
            persist_content,
            permissions_ui,
            advisory_ui,
            verification_ui,
            advisory_profiles,
            trusted_session,
            bind,
        } => {
            let profile_context = if provider.is_none() {
                let id = profile.as_deref().or(profiles.default_profile.as_deref());
                id.and_then(|id| profiles.profiles.get(id))
                    .map(|configured| {
                        configured.view(if profile.is_some() {
                            "explicit_profile"
                        } else {
                            "default_profile"
                        })
                    })
            } else {
                None
            };
            let controls = profiles.controls(provider.as_deref(), profile.as_deref());
            let provider = match profiles.select(provider.as_deref(), profile.as_deref()) {
                Ok(provider) => provider,
                Err(error) => return report_error(error),
            };
            match chat_provider(
                &registry,
                &provider,
                profile_context,
                controls,
                workspace,
                database,
                persist_content,
                permissions_ui,
                advisory_ui,
                verification_ui,
                advisory_profiles.then_some(&profiles),
                trusted_session,
                bind,
                Some(profile_store),
                Some(agent_store),
                &profiles,
            )
            .await
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => report_error(error),
            }
        }
    }
}

async fn serve_live_ui(database: &Path, bind: SocketAddr, limit: usize) -> Result<()> {
    let store = SqliteEventStore::open_read_only(database)?;
    let server = LiveUiServer::bind(store, bind, limit).await?;
    println!("Urdia live timeline: {}", server.access_url());
    println!("Loopback only. Keep this URL private. Press Ctrl-C to stop.");
    server.serve().await
}

async fn generate_timeline(database: &Path, output: &Path, limit: usize) -> Result<()> {
    let database = std::fs::canonicalize(database).map_err(|error| {
        UrdiaError::Store(format!(
            "open timeline database {}: {error}",
            database.display()
        ))
    })?;
    let output_identity = canonical_output_identity(output)?;
    if same_file(&database, &output_identity)? {
        return Err(UrdiaError::InvalidInput(
            "timeline output must not replace the event database".to_owned(),
        ));
    }
    let store = SqliteEventStore::open_read_only(&database)?;
    let sessions = load_timeline(&store, limit).await?;
    let html = render_timeline(&sessions);
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| UrdiaError::Ui(format!("create timeline directory: {error}")))?;
    }
    let output_directory = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".urdia-timeline-")
        .tempfile_in(output_directory)
        .map_err(|error| UrdiaError::Ui(format!("create temporary timeline HTML: {error}")))?;
    #[cfg(unix)]
    temporary
        .as_file()
        .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(|error| UrdiaError::Ui(format!("protect temporary timeline HTML: {error}")))?;
    temporary
        .write_all(html.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| UrdiaError::Ui(format!("write timeline HTML: {error}")))?;
    let persisted = temporary
        .persist(output)
        .map_err(|error| UrdiaError::Ui(format!("replace timeline HTML: {}", error.error)))?;
    persisted
        .sync_all()
        .map_err(|error| UrdiaError::Ui(format!("sync timeline HTML: {error}")))?;
    println!("Urdia timeline: {}", output.display());
    Ok(())
}

fn distinct_settings(a: &Path, b: &Path) -> Result<bool> {
    let identity = |path: &Path| -> Result<PathBuf> {
        let name = path
            .file_name()
            .ok_or_else(|| UrdiaError::InvalidInput("invalid settings path".into()))?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Ok(std::fs::canonicalize(parent)
            .map_err(|_| UrdiaError::InvalidInput("invalid settings directory".into()))?
            .join(name))
    };
    if identity(a)? == identity(b)? {
        return Ok(false);
    }
    if a.exists() && b.exists() {
        return same_file(a, b).map(|same| !same);
    }
    Ok(true)
}

fn same_file(database: &Path, output: &Path) -> Result<bool> {
    if database == output {
        return Ok(true);
    }
    if !output.exists() {
        return Ok(false);
    }
    let database_metadata = std::fs::metadata(database)
        .map_err(|error| UrdiaError::Ui(format!("inspect event database: {error}")))?;
    let output_metadata = std::fs::metadata(output)
        .map_err(|error| UrdiaError::Ui(format!("inspect timeline output: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(database_metadata.dev() == output_metadata.dev()
            && database_metadata.ino() == output_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = (database_metadata, output_metadata);
        Ok(false)
    }
}

fn canonical_output_identity(output: &Path) -> Result<PathBuf> {
    if output.exists() {
        return std::fs::canonicalize(output)
            .map_err(|error| UrdiaError::Ui(format!("inspect timeline output: {error}")));
    }
    let _name = output.file_name().ok_or_else(|| {
        UrdiaError::InvalidInput("timeline output must name an HTML file".to_owned())
    })?;
    if output.is_absolute() {
        Ok(output.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(output))
            .map_err(|error| UrdiaError::Ui(format!("inspect current directory: {error}")))
    }
}

fn report_error(error: UrdiaError) -> ExitCode {
    eprintln!("urdia: {error}");
    ExitCode::FAILURE
}

#[cfg(test)]
fn build_registry(config_path: Option<&Path>) -> Result<ProviderRegistry> {
    build_configuration(config_path).map(|(registry, _)| registry)
}

#[cfg(test)]
fn build_configuration(config_path: Option<&Path>) -> Result<(ProviderRegistry, ProfileCatalog)> {
    build_configuration_with_agents(config_path, Vec::new())
}
fn build_configuration_with_agents(
    config_path: Option<&Path>,
    agents: Vec<urdia_ui::AgentDraft>,
) -> Result<(ProviderRegistry, ProfileCatalog)> {
    let mut registry = ProviderRegistry::new();
    registry.register_owned(QwenProvider::new())?;
    for agent in agents {
        registry.register(Arc::new(AcpProvider::with_metadata(
            agent.id,
            agent.name,
            AcpCommand::new(agent.command).args(agent.args),
        )))?;
    }
    let mut profiles = ProfileCatalog::default();

    if let Some(path) = config_path {
        let source = std::fs::read_to_string(path).map_err(|error| {
            UrdiaError::InvalidInput(format!("read provider config {}: {error}", path.display()))
        })?;
        let config: UrdiaConfig = toml::from_str(&source).map_err(|error: toml::de::Error| {
            let location = error.span().map_or_else(
                || "unknown location".to_owned(),
                |span| format!("byte {}", span.start),
            );
            UrdiaError::InvalidInput(format!(
                "parse provider config {}: invalid TOML near {location}",
                path.display()
            ))
        })?;
        for definition in config.providers {
            let command = AcpCommand::new(definition.command).args(definition.args);
            registry.register(Arc::new(AcpProvider::with_metadata(
                definition.id,
                definition.name,
                command,
            )))?;
        }
        profiles = ProfileCatalog::new(config.profiles, config.default_profile, &registry)?;
    }

    Ok((registry, profiles))
}

fn doctor(registry: &ProviderRegistry) {
    println!("Urdia doctor");
    println!("  permissions: deny by default; one-shot browser opt-in available");
    println!("  transport: ACP v1 over stdio");
    println!("  registered providers: {}", registry.len());
    for info in registry.list() {
        println!(
            "  {}: {} ({})",
            info.id,
            availability_label(info.availability),
            info.availability_detail
                .as_deref()
                .unwrap_or("no diagnostic")
        );
    }
    println!("  provider credentials: not inspected");
}

fn print_providers(registry: &ProviderRegistry) {
    println!("ID\tSTATUS\tAUTH\tINFERENCE\tTRANSPORT\tNAME");
    for info in registry.list() {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            info.id,
            availability_label(info.availability),
            authentication_label(info.authentication),
            inference_label(info.inference),
            info.protocols.join(","),
            info.name
        );
    }
}

const MAX_ROUTE_STATE_BYTES: usize = 128 * 1_024;
const MAX_EVALUATION_BYTES: usize = 2 * 1_024 * 1_024;

#[derive(Debug, Serialize)]
struct RouteCliOutput {
    advisory: bool,
    selected_provider: Option<String>,
    result: urdia_routing::RouteResult,
}

async fn recommend_route(
    registry: &ProviderRegistry,
    objective: String,
    router: RouterKind,
    requested_candidates: Vec<String>,
    state_path: Option<&Path>,
    min_confidence: Option<f32>,
    json_output: bool,
) -> Result<()> {
    let infos = selected_provider_infos(registry, &requested_candidates)?;
    let state = match state_path {
        Some(path) => read_bounded_json(path, MAX_ROUTE_STATE_BYTES, "routing state")?,
        None => json!({}),
    };
    let mut provider_by_candidate = BTreeMap::new();
    let candidates = infos
        .into_iter()
        .enumerate()
        .map(|(index, info)| {
            let candidate_id = format!("route_{index:03}");
            provider_by_candidate.insert(candidate_id.clone(), info.id.clone());
            RouteCandidate {
                id: candidate_id,
                description: provider_route_description(&info),
            }
        })
        .collect();
    let resolver = route_resolver(router, min_confidence)?;
    let result = resolver
        .resolve(RouteRequest {
            objective,
            state,
            candidates,
        })
        .await?;
    let selected_provider = result
        .selected
        .as_ref()
        .and_then(|candidate| provider_by_candidate.get(candidate))
        .cloned();
    let output = RouteCliOutput {
        advisory: true,
        selected_provider,
        result,
    };
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|_| UrdiaError::InvalidInput("serialize routing result".to_owned()))?
        );
    } else {
        println!("Urdia advisory route");
        println!("  router: {}", output.result.decision_provider);
        match &output.selected_provider {
            Some(provider) => println!("  recommended provider: {provider}"),
            None => println!(
                "  fallback: {}",
                output
                    .result
                    .fallback_reason
                    .map_or("unspecified".to_owned(), |reason| format!("{reason:?}")
                        .to_lowercase())
            ),
        }
        if let Some(confidence) = output.result.confidence {
            println!("  confidence: {confidence:.3}");
        }
        println!("  advisory only: no agent started and no permission granted");
    }
    Ok(())
}

fn selected_provider_infos(
    registry: &ProviderRegistry,
    requested: &[String],
) -> Result<Vec<ProviderInfo>> {
    if requested.is_empty() {
        let available = registry
            .list()
            .into_iter()
            .filter(|info| info.availability.is_available())
            .collect::<Vec<_>>();
        if available.is_empty() {
            return Err(UrdiaError::InvalidInput(
                "no available providers can be routed".to_owned(),
            ));
        }
        return Ok(available);
    }
    let mut seen = std::collections::BTreeSet::new();
    requested
        .iter()
        .map(|id| {
            if !seen.insert(id.as_str()) {
                return Err(UrdiaError::InvalidInput(format!(
                    "duplicate routing provider `{id}`"
                )));
            }
            Ok(registry.resolve_available(id)?.info())
        })
        .collect()
}

fn provider_route_description(info: &ProviderInfo) -> String {
    format!(
        "Use the registered agent provider named {}. Transport: {}. Host-visible capabilities: {}. Authentication: {}. Inference location: {}.",
        info.name,
        if info.protocols.is_empty() {
            "unspecified".to_owned()
        } else {
            info.protocols.join(", ")
        },
        if info.capabilities.is_empty() {
            "unspecified".to_owned()
        } else {
            info.capabilities.join(", ")
        },
        authentication_label(info.authentication),
        inference_label(info.inference),
    )
}

fn route_resolver(router: RouterKind, min_confidence: Option<f32>) -> Result<RouteResolver> {
    let provider: Arc<dyn urdia_core::DecisionProvider> = match router {
        RouterKind::Deterministic => Arc::new(FirstCandidateDecisionProvider::new("deterministic")),
        RouterKind::Jev => Arc::new(JevDecisionProvider::from_env()?),
        RouterKind::Laya => Arc::new(LayaDecisionProvider::from_env()?),
        RouterKind::Typellm => Arc::new(TypeLlmDecisionProvider::from_env()?),
    };
    RouteResolver::new(provider).with_policy(RoutePolicy { min_confidence })
}

/// Host-owned adapters and candidates; no keys enter a UI response.
struct BrowserAdvisory {
    engines: BTreeMap<String, Option<RouteResolver>>,
    laya: local_laya::LocalLaya,
    jev: local_jev::LocalJev,
    candidates: Vec<RouteCandidate>,
    providers: BTreeMap<String, String>,
    profiles: BTreeMap<String, ConversationProfile>,
    state: Value,
    editor: Option<Arc<profile_store::ProfileStore>>,
    agent_editor: Option<Arc<agent_store::AgentStore>>,
    control_probe: Option<Arc<control_probe::ControlProbe>>,
    execution_profiles: BTreeMap<String, ExecutionProfile>,
    execution_providers: BTreeMap<String, Arc<dyn urdia_core::AgentProvider>>,
    execution_scope: Option<(String, String)>,
}

impl BrowserAdvisory {
    fn new(registry: &ProviderRegistry) -> Result<Self> {
        Self::build(registry, true)
    }

    fn build(registry: &ProviderRegistry, configured: bool) -> Result<Self> {
        let infos = selected_provider_infos(registry, &[])?;
        let mut providers = BTreeMap::new();
        let candidates = infos
            .into_iter()
            .enumerate()
            .map(|(index, info)| {
                let id = format!("route_{index:03}");
                providers.insert(id.clone(), info.id.clone());
                RouteCandidate {
                    id,
                    description: provider_route_description(&info),
                }
            })
            .collect();
        let engines = [
            ("deterministic", RouterKind::Deterministic),
            ("typellm", RouterKind::Typellm),
        ]
        .into_iter()
        .map(|(id, kind)| {
            (
                id.to_owned(),
                if configured {
                    route_resolver(kind, None).ok()
                } else {
                    None
                },
            )
        })
        .collect();
        Ok(Self {
            engines,
            jev: local_jev::LocalJev::new(configured),
            laya: if configured {
                local_laya::LocalLaya::from_environment()
            } else {
                local_laya::LocalLaya::default()
            },
            candidates,
            providers,
            profiles: BTreeMap::new(),
            state: json!({}),
            editor: None,
            agent_editor: None,
            control_probe: None,
            execution_profiles: BTreeMap::new(),
            execution_providers: BTreeMap::new(),
            execution_scope: None,
        })
    }

    fn local_only(registry: &ProviderRegistry) -> Result<Self> {
        let mut prepared = Self::build(registry, false)?;
        prepared.engines.clear();
        prepared.jev = local_jev::LocalJev::new(false);
        prepared.laya = local_laya::LocalLaya::default();
        Ok(prepared)
    }

    fn for_profiles(registry: &ProviderRegistry, catalog: &ProfileCatalog) -> Result<Self> {
        Self::prepared_profiles(registry, catalog, true)
    }

    fn prepared_profiles(
        registry: &ProviderRegistry,
        catalog: &ProfileCatalog,
        configured: bool,
    ) -> Result<Self> {
        let mut prepared = Self::build(registry, configured)?;
        if !configured {
            prepared.engines.clear();
        }
        prepared.candidates.clear();
        prepared.providers.clear();
        let mut profile_state = BTreeMap::new();
        for profile in catalog.profiles.values() {
            let info = registry.resolve(&profile.provider)?.info();
            if !info.availability.is_available() {
                continue;
            }
            let id = format!("route_{:03}", prepared.candidates.len());
            prepared
                .execution_profiles
                .insert(profile.id.clone(), profile.clone());
            prepared
                .execution_providers
                .insert(profile.id.clone(), registry.resolve(&profile.provider)?);
            prepared
                .providers
                .insert(id.clone(), profile.provider.clone());
            prepared
                .profiles
                .insert(id.clone(), profile.view("advisory"));
            profile_state.insert(
                id.clone(),
                json!({
                    "declared_configuration": profile,
                    "executable_available": true,
                    "model_effort_verified": false,
                    "authentication": authentication_label(info.authentication),
                    "inference_location": inference_label(info.inference),
                    "measured_task_quality": null, "measured_latency": null, "measured_cost": null,
                }),
            );
            prepared.candidates.push(RouteCandidate {
                description: format!("Recommend the host-declared execution profile in the execution_profiles map nested in the request context, at key {id}. Model and effort are unverified operator labels, not evidence of effective settings or task quality. Executable discovery does not verify ACP compatibility or credentials. Compare available evidence; do not invent performance, price or capabilities. This advice starts no work and grants no permissions."),
                id,
            });
        }
        if prepared.candidates.is_empty() {
            return Err(UrdiaError::InvalidInput("profile advice requires at least one configured profile with an available executable".to_owned()));
        }
        prepared.state = json!({
            "schema": "urdia-declared-profile-advice-v1",
            "execution_profiles": profile_state,
            "operator_default_profile": catalog.default_profile,
            "automatic_execution": false,
            "unverified_labels_are_not_capability_evidence": true,
        });
        if serde_json::to_vec(&prepared.state)
            .map_err(|_| UrdiaError::InvalidInput("invalid profile advice state".to_owned()))?
            .len()
            > MAX_ROUTE_STATE_BYTES
        {
            return Err(UrdiaError::InvalidInput(
                "profile advice state exceeds its size limit".to_owned(),
            ));
        }
        Ok(prepared)
    }
}

#[async_trait::async_trait]
impl urdia_ui::AdvisoryProvider for BrowserAdvisory {
    fn automatic_execution_available(&self) -> bool {
        self.execution_scope.is_some()
            && self.execution_profiles.values().any(|profile| {
                profile.session_controls.as_ref().is_some_and(|controls| {
                    controls.model.is_some()
                        && (profile.effort.is_none() || controls.effort.is_some())
                }) && self
                    .execution_providers
                    .get(&profile.id)
                    .is_some_and(|provider| provider.info().availability.is_available())
            })
    }
    fn candidate_kind(&self) -> &'static str {
        if self.profiles.is_empty() {
            "provider"
        } else {
            "profile"
        }
    }

    async fn inspect_controls(
        &self,
        request: urdia_ui::ControlProbeRequest,
    ) -> std::result::Result<urdia_ui::ControlProbeResult, urdia_ui::ControlProbeError> {
        self.control_probe
            .as_ref()
            .ok_or(urdia_ui::ControlProbeError::Unavailable)?
            .inspect(request)
            .await
    }
    fn agent_catalog(&self) -> Option<urdia_ui::AgentCatalogSnapshot> {
        self.agent_editor.as_ref().map(|editor| editor.snapshot())
    }
    fn register_agent(
        &self,
        request: urdia_ui::RegisterAgentRequest,
    ) -> std::result::Result<urdia_ui::AgentCatalogSnapshot, urdia_ui::CatalogError> {
        let definition = request.agent.clone();
        let snapshot = self
            .agent_editor
            .as_ref()
            .ok_or(urdia_ui::CatalogError::Disabled)?
            .register(request)?;
        // Only after persistence succeeds: append to editor/inspection views, never execution.
        let provider: Arc<dyn urdia_core::AgentProvider> = Arc::new(AcpProvider::with_metadata(
            definition.id,
            definition.name,
            AcpCommand::new(definition.command).args(definition.args),
        ));
        if let Some(editor) = &self.editor {
            editor
                .stage_provider(provider.clone())
                .map_err(|_| urdia_ui::CatalogError::WriteFailed)?;
        }
        if let Some(probe) = &self.control_probe {
            probe.stage_provider(provider);
        }
        Ok(snapshot)
    }
    fn catalog(&self) -> Option<urdia_ui::CatalogSnapshot> {
        self.editor.as_ref().map(|editor| editor.snapshot())
    }
    fn save_profile(
        &self,
        request: urdia_ui::SaveProfileRequest,
    ) -> std::result::Result<urdia_ui::CatalogSnapshot, urdia_ui::CatalogError> {
        self.editor
            .as_ref()
            .ok_or(urdia_ui::CatalogError::Disabled)?
            .save(request)
    }
    fn execution_target(
        &self,
        result: &urdia_ui::AdvisoryResult,
    ) -> Option<urdia_ui::ExecutionTarget> {
        let (workspace_name, permission_mode) = self.execution_scope.as_ref()?;
        if result.fallback_reason.is_some() {
            return None;
        }
        let selected = result.selected_profile.as_ref()?;
        let profile = self.execution_profiles.get(&selected.id)?;
        if result.selected_provider.as_deref() != Some(&profile.provider) {
            return None;
        }
        if !self
            .execution_providers
            .get(&profile.id)?
            .info()
            .availability
            .is_available()
        {
            return None;
        }
        let controls = profile.session_controls.as_ref()?;
        let model_selector = controls.model.clone()?;
        if profile.effort.is_some() && controls.effort.is_none() {
            return None;
        }
        Some(urdia_ui::ExecutionTarget {
            profile: profile.view("operator-confirmed"),
            provider_id: profile.provider.clone(),
            workspace_name: workspace_name.clone(),
            model_selector,
            effort_selector: controls.effort.clone(),
            permission_mode: permission_mode.clone(),
        })
    }

    fn engines(&self) -> Vec<urdia_ui::AdvisoryEngine> {
        self.engines
            .iter()
            .map(|(id, resolver)| urdia_ui::AdvisoryEngine {
                id: id.clone(),
                configured: resolver.is_some(),
                remote: id == "jev",
            })
            .chain(std::iter::once(urdia_ui::AdvisoryEngine {
                id: "jev".into(),
                configured: self.jev.resolver().is_some(),
                remote: true,
            }))
            .chain(std::iter::once(urdia_ui::AdvisoryEngine {
                id: "laya".into(),
                configured: self.laya.resolver().is_some(),
                remote: false,
            }))
            .collect()
    }

    fn jev_connection(&self) -> Option<urdia_ui::JevConnectionView> {
        Some(self.jev.view())
    }
    async fn configure_jev(
        &self,
        request: urdia_ui::JevConnectionRequest,
    ) -> std::result::Result<urdia_ui::JevConnectionView, urdia_ui::JevConnectionError> {
        self.jev.configure(request).await
    }

    fn laya_connection(&self) -> Option<urdia_ui::LayaConnectionView> {
        Some(self.laya.view())
    }
    async fn configure_laya(
        &self,
        request: urdia_ui::LayaConnectionRequest,
    ) -> std::result::Result<urdia_ui::LayaConnectionView, urdia_ui::LayaConnectionError> {
        self.laya.configure(request).await
    }

    async fn recommend(&self, engine: &str, objective: String) -> Option<urdia_ui::AdvisoryResult> {
        let resolver = if engine == "jev" {
            if self.profiles.is_empty() {
                self.jev.resolver()?
            } else {
                self.jev.profile_resolver()?
            }
        } else if engine == "laya" {
            self.laya.resolver()?
        } else {
            self.engines.get(engine)?.as_ref()?.clone()
        };
        let result = resolver
            .resolve(RouteRequest {
                objective,
                state: self.state.clone(),
                candidates: self.candidates.clone(),
            })
            .await
            .ok()?;
        Some(urdia_ui::AdvisoryResult {
            selected_profile: result
                .selected
                .as_ref()
                .and_then(|id| self.profiles.get(id))
                .cloned(),
            selected_provider: result
                .selected
                .as_ref()
                .and_then(|id| self.providers.get(id))
                .cloned(),
            probabilities: result
                .probabilities
                .iter()
                .filter_map(|(id, probability)| {
                    self.profiles
                        .get(id)
                        .map(|profile| profile.id.clone())
                        .or_else(|| self.providers.get(id).cloned())
                        .map(|label| (label, *probability))
                })
                .collect(),
            review_probability: result
                .probabilities
                .get(urdia_core::ESCALATION_CANDIDATE_ID)
                .copied(),
            confidence: result.confidence,
            probability_semantics: format!("{:?}", result.probability_semantics),
            fallback_reason: result.fallback_reason.map(|reason| {
                match reason {
                    urdia_routing::RouteFallbackReason::ProviderError => "provider_error",
                    urdia_routing::RouteFallbackReason::NoSelection => "no_selection",
                    urdia_routing::RouteFallbackReason::LowConfidence => "low_confidence",
                    urdia_routing::RouteFallbackReason::InvalidDecision => "invalid_decision",
                }
                .to_owned()
            }),
        })
    }
}

fn read_bounded_json(path: &Path, limit: usize, label: &str) -> Result<Value> {
    let file = std::fs::File::open(path).map_err(|error| {
        UrdiaError::InvalidInput(format!("read {label} {}: {error}", path.display()))
    })?;
    let byte_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::with_capacity(limit.min(64 * 1_024));
    file.take(byte_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            UrdiaError::InvalidInput(format!("read {label} {}: {error}", path.display()))
        })?;
    if bytes.len() > limit {
        return Err(UrdiaError::InvalidInput(format!(
            "{label} exceeds its size limit"
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| UrdiaError::InvalidInput(format!("{label} is not valid JSON")))
}

const fn availability_label(value: ProviderAvailability) -> &'static str {
    match value {
        ProviderAvailability::Available => "available",
        ProviderAvailability::Unavailable => "unavailable",
    }
}

const fn authentication_label(value: AuthenticationState) -> &'static str {
    match value {
        AuthenticationState::Configured => "configured",
        AuthenticationState::Missing => "missing",
        AuthenticationState::Unknown => "unknown",
    }
}

const fn inference_label(value: InferenceLocation) -> &'static str {
    match value {
        InferenceLocation::Local => "local",
        InferenceLocation::Remote => "remote",
        InferenceLocation::Mixed => "mixed",
        InferenceLocation::Unknown => "unknown",
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_provider(
    registry: &ProviderRegistry,
    provider_id: &str,
    controls: urdia_core::SessionControls,
    workspace: PathBuf,
    prompt: String,
    database: PathBuf,
    persist_content: bool,
    trusted_session: bool,
    permissions_bind: Option<SocketAddr>,
) -> Result<()> {
    // Selection and availability checks intentionally happen before workspace
    // or database side effects.
    let provider = registry.resolve_available(provider_id)?;
    let provider_info = provider.info();
    let workspace = WorkspacePath::new(workspace)?;
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            UrdiaError::Store(format!("create event database directory: {error}"))
        })?;
    }
    let store = SqliteEventStore::open(&database)?;
    let (permission_broker, live_ui) = if let Some(bind) = permissions_bind {
        let broker = if trusted_session {
            InteractivePermissionBroker::with_trusted_session(provider_info.name.clone())
        } else {
            InteractivePermissionBroker::new()
        };
        let reader = SqliteEventStore::open_read_only(&database)?;
        let server = LiveUiServer::bind_interactive(reader, bind, 50, broker.clone()).await?;
        println!("Urdia permission UI: {}", server.access_url());
        println!("Loopback only. Keep this URL private; requests expire automatically.");
        (Some(broker), Some(ActiveLiveUi::start(server)))
    } else {
        (None, None)
    };
    let receipt_sink = Arc::new(SqlitePermissionReceiptSink {
        store: store.clone(),
    });
    let run_result = async {
        let mut session_request = SessionRequest::new(workspace.clone())
            .controls(controls)
            .permission_receipt_sink(receipt_sink);
        if let Some(broker) = &permission_broker {
            session_request = session_request.permission_broker(Arc::new(broker.clone()));
        }
        let session = provider.start_session(session_request).await?;
        let session_id = session.id().clone();
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::SessionStarted,
                provider_event_data(&provider_info, &workspace),
            ))
            .await?;
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::PromptSubmitted,
                if persist_content {
                    json!({"text": prompt})
                } else {
                    json!({"content_persisted": false, "characters": prompt.chars().count()})
                },
            ))
            .await?;
        let result = match session.prompt(Prompt::new(prompt)).await {
            Ok(result) => result,
            Err(error) => {
                store
                    .append(Event::new(
                        session_id.clone(),
                        EventKind::Error,
                        json!({"stage": "prompt", "failure_kind":provider_failure_kind(&error), "details_persisted": false}),
                    ))
                    .await?;
                store
                    .append(Event::new(
                        session_id,
                        EventKind::SessionFinished,
                        json!({"status": "failed"}),
                    ))
                    .await?;
                return Err(error);
            }
        };
        if !result.text.is_empty() {
            println!("{}", result.text);
        }
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::AgentText,
                if persist_content {
                    json!({"text": result.text, "stop_reason": result.stop_reason})
                } else {
                    json!({
                        "content_persisted": false,
                        "characters": result.text.chars().count(),
                        "stop_reason": result.stop_reason
                    })
                },
            ))
            .await?;
        store
            .append(Event::new(
                session_id,
                EventKind::SessionFinished,
                json!({"status": "completed"}),
            ))
            .await?;
        Ok(())
    }
    .await;

    let ui_result = match live_ui {
        Some(live_ui) => live_ui.stop().await,
        None => Ok(()),
    };
    run_result.and(ui_result)
}

#[allow(clippy::too_many_arguments)]
async fn chat_provider(
    registry: &ProviderRegistry,
    provider_id: &str,
    profile_context: Option<ConversationProfile>,
    controls: urdia_core::SessionControls,
    workspace: PathBuf,
    database: PathBuf,
    persist_content: bool,
    permissions_ui: bool,
    advisory_ui: bool,
    verification_ui: bool,
    advisory_profiles: Option<&ProfileCatalog>,
    trusted_session: bool,
    bind: SocketAddr,
    editor: Option<Arc<profile_store::ProfileStore>>,
    agent_editor: Option<Arc<agent_store::AgentStore>>,
    session_profiles: &ProfileCatalog,
) -> Result<()> {
    // Resolve the provider and workspace before creating durable state or a
    // listening socket.
    if !bind.ip().is_loopback() {
        return Err(UrdiaError::InvalidInput(format!(
            "UI bind address must be loopback, received {}",
            bind.ip()
        )));
    }
    if verification_ui && !verification::available() {
        return Err(UrdiaError::InvalidInput(
            "project verification requires Linux/WSL, /usr/bin/python3 and /bin/kill".into(),
        ));
    }
    let provider = registry.resolve_available(provider_id)?;
    let provider_info = provider.info();
    let workspace = WorkspacePath::new(workspace)?;
    let has_available_profiles = session_profiles.profiles.values().any(|p| {
        registry
            .resolve(&p.provider)
            .is_ok_and(|p| p.info().availability.is_available())
    });
    let mut prepared_advice = if advisory_ui {
        Some(match advisory_profiles {
            Some(catalog) => BrowserAdvisory::for_profiles(registry, catalog)?,
            None if has_available_profiles => {
                BrowserAdvisory::for_profiles(registry, session_profiles)?
            }
            None => BrowserAdvisory::new(registry)?,
        })
    } else {
        // Browser-connected engines can compare saved profiles without loading
        // external credentials or contacting an engine during startup.
        Some(if has_available_profiles {
            BrowserAdvisory::prepared_profiles(registry, session_profiles, false)?
        } else {
            BrowserAdvisory::local_only(registry)?
        })
    };
    if let Some(advice) = &mut prepared_advice {
        advice.editor = editor;
        advice.agent_editor = agent_editor;
    }
    let workspace_name = workspace.as_path().file_name().map_or_else(
        || "Project".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    if !trusted_session && let Some(advice) = &mut prepared_advice {
        advice.execution_scope = Some((
            workspace_name.clone(),
            if permissions_ui {
                "one-shot approval"
            } else {
                "deny by default"
            }
            .into(),
        ));
    }
    let mut execution_profiles = prepared_advice
        .as_ref()
        .map(|a| a.execution_profiles.clone())
        .unwrap_or_default();
    let mut execution_providers = prepared_advice
        .as_ref()
        .map(|a| a.execution_providers.clone())
        .unwrap_or_default();
    let mut profile_choices = BTreeMap::new();
    if !trusted_session {
        for profile in session_profiles.profiles.values() {
            let Some(controls) = &profile.session_controls else {
                continue;
            };
            if controls.model.is_none() || (profile.effort.is_some() && controls.effort.is_none()) {
                continue;
            }
            let Ok(provider) = registry.resolve_available(&profile.provider) else {
                continue;
            };
            let info = provider.info();
            let mut choice_view = profile.view("manual-next-task");
            choice_view.model = controls
                .model
                .clone()
                .expect("eligible profile has a model");
            choice_view.effort = controls.effort.clone();
            profile_choices.insert(
                profile.id.clone(),
                ConversationContext {
                    provider_id: info.id,
                    provider_name: info.name,
                    workspace_name: workspace_name.clone(),
                    profile: Some(choice_view),
                    startup_controls: None,
                },
            );
            execution_profiles.insert(profile.id.clone(), profile.clone());
            execution_providers.insert(profile.id.clone(), provider);
        }
    }
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            UrdiaError::Store(format!("create event database directory: {error}"))
        })?;
    }
    let store = SqliteEventStore::open(&database)?;
    let receipt_sink = Arc::new(SqlitePermissionReceiptSink {
        store: store.clone(),
    });
    if let Some(advice) = &mut prepared_advice {
        advice.control_probe = Some(Arc::new(control_probe::ControlProbe::new(
            registry,
            workspace.clone(),
            receipt_sink.clone(),
        )));
    }
    let permission_broker = permissions_ui.then(|| {
        if trusted_session {
            InteractivePermissionBroker::with_trusted_session(provider_info.name.clone())
        } else {
            InteractivePermissionBroker::new()
        }
    });
    let mut session_request = SessionRequest::new(workspace.clone())
        .controls(controls)
        .permission_receipt_sink(receipt_sink.clone());
    if let Some(broker) = &permission_broker {
        session_request = session_request.permission_broker(Arc::new(broker.clone()));
    }
    let session: Arc<dyn AgentSession> =
        Arc::from(provider.start_session(session_request.clone()).await?);
    let session_id = session.id().clone();
    store
        .append(Event::new(
            session_id.clone(),
            EventKind::SessionStarted,
            provider_event_data(&provider_info, &workspace),
        ))
        .await?;

    let (conversation, mut commands) = ConversationHub::with_context(ConversationContext {
        provider_id: provider_info.id.clone(),
        provider_name: provider_info.name.clone(),
        profile: profile_context,
        startup_controls: session.startup_controls(),
        workspace_name: workspace.as_path().file_name().map_or_else(
            || "Project".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
    });
    if verification_ui && verification::available() {
        conversation.enable_verification();
    }
    if !trusted_session {
        conversation.enable_reconnection();
    }
    if result_source::available() {
        conversation.enable_result_preview();
    }
    conversation.configure_profile_selection(profile_choices);
    conversation.set_live_controls(session.live_controls());
    let reader = SqliteEventStore::open_read_only(&database)?;
    let server = if let Some(prepared_advice) = prepared_advice {
        LiveUiServer::bind_workspace(
            reader,
            bind,
            conversation.clone(),
            permission_broker.clone(),
            urdia_ui::AdvisoryHub::new(Arc::new(prepared_advice)),
        )
        .await?
    } else {
        match permission_broker.clone() {
            Some(broker) => {
                LiveUiServer::bind_conversation_interactive(
                    reader,
                    bind,
                    50,
                    conversation.clone(),
                    broker,
                )
                .await?
            }
            None => LiveUiServer::bind_conversation(reader, bind, 50, conversation.clone()).await?,
        }
    };
    println!("Urdia conversation: {}", server.access_url());
    println!("Loopback only. Keep this URL private. Press Ctrl-C to stop.");
    let live_ui = ActiveLiveUi::start(server);

    let mut session = Some(session);
    let mut session_id = Some(session_id);
    let run_result = async {
        let mut turns = JoinSet::new();
        let mut active_cancel: Option<(Uuid, mpsc::Sender<()>)> = None;
        let mut controls_tick = tokio::time::interval(std::time::Duration::from_millis(250));
        let shutdown = tokio::signal::ctrl_c();
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = controls_tick.tick() => conversation.set_live_controls(session.as_ref().and_then(|s| s.live_controls())),
                _ = &mut shutdown => {
                    if let Some(turn_id) = conversation.snapshot().active_turn_id {
                        let signalled = active_cancel
                            .as_ref()
                            .filter(|(active_id, _)| *active_id == turn_id)
                            .is_some_and(|(_, sender)| sender.try_send(()).is_ok());
                        if !signalled {
                            if let Some(session) = &session { let _ = session.cancel().await; }
                            let _ = conversation.cancel(turn_id);
                        }
                    }
                    if conversation.snapshot().verification.as_ref().is_some_and(|v| v.active) {
                        if let Some((_, sender)) = &active_cancel { let _ = sender.try_send(()); }
                        if let Some(job) = conversation.snapshot().verification.and_then(|v| v.latest) { let _ = conversation.cancel_verification(job.request_id); }
                    }
                    while let Some(joined) = turns.join_next().await {
                        let (_, turn_result) = joined.map_err(|error| {
                            UrdiaError::Provider(format!("join conversation turn: {error}"))
                        })?;
                        turn_result?;
                    }
                    break;
                }
                command = commands.recv() => match command {
                    Some(ConversationCommand::Prompt { turn_id, text }) => {
                        if !begin_conversation_turn(&conversation, turn_id)? {
                            continue;
                        }
                        let (cancel_sender, cancel_receiver) = mpsc::channel(1);
                        active_cancel = Some((turn_id, cancel_sender));
                        let Some(turn_session) = session.as_ref().map(Arc::clone) else {
                            conversation.fail(turn_id, "Reconnect the original agent before sending another task.".into()).map_err(|_| UrdiaError::Provider("record disconnected turn".into()))?;
                            continue;
                        };
                        let turn_store = store.clone();
                        let turn_session_id = turn_session.id().clone();
                        let turn_conversation = conversation.clone();
                        turns.spawn(async move {
                            let result = run_chat_turn(ChatTurn {
                                session: turn_session,
                                store: turn_store,
                                session_id: turn_session_id,
                                conversation: turn_conversation,
                                turn_id,
                                text,
                                persist_content,
                                cancel_receiver,
                            })
                            .await;
                            (turn_id, result)
                        });
                    }
                    Some(ConversationCommand::Recommended { turn_id, text, profile_id }) => {
                        if !begin_conversation_turn(&conversation, turn_id)? { continue; }
                        let (cancel_sender, cancel_receiver) = mpsc::channel(1);
                        active_cancel = Some((turn_id, cancel_sender));
                        let profile = execution_profiles.get(&profile_id).cloned();
                        let provider = execution_providers.get(&profile_id).cloned();
                        let task = RecommendedTurn {
                            profile, provider, workspace: workspace.clone(), store: store.clone(),
                            receipt_sink: receipt_sink.clone(), broker: permission_broker.clone(),
                            conversation: conversation.clone(), turn_id, text, persist_content, cancel_receiver,
                        };
                        turns.spawn(async move { (turn_id, run_recommended_turn(task).await) });
                    }
                    Some(ConversationCommand::Reconnect { revision }) => {
                        // Admission has atomically excluded prompts, verification and selection.
                        // Drain finished tasks so no old session Arc survives replacement.
                        while let Some(joined) = turns.join_next().await {
                            let (_, result) = joined.map_err(|_| UrdiaError::Provider("join finished task before reconnection".into()))?;
                            result?;
                        }
                        active_cancel = None;
                        if trusted_session || permission_broker.as_ref().is_some_and(|b| !b.pending().is_empty() || b.trusted_state().is_some()) {
                            conversation.finish_reconnection(revision, Err(urdia_core::ProviderFailureKind::Unknown));
                            continue;
                        }
                        session.take(); // release the old process; never replay a prompt
                        conversation.set_live_controls(None);
                        if let Some(old_id) = session_id.take() {
                            store.append(Event::new(old_id, EventKind::SessionFinished,
                                json!({"status":"replaced", "context_reset":true}))).await?;
                        }
                        let started = tokio::select! {
                            _ = &mut shutdown => break,
                            result = start_reconnected_session(provider.as_ref(), session_request.clone()) => result,
                        };
                        match started {
                            Ok(new_session) => {
                                let new_id = new_session.id().clone();
                                let mut data = provider_event_data(&provider_info, &workspace);
                                data["reconnection"] = json!(true);
                                data["context_reset"] = json!(true);
                                store.append(Event::new(new_id.clone(), EventKind::SessionStarted, data)).await?;
                                conversation.finish_reconnection(revision, Ok(new_session.startup_controls()));
                                conversation.set_live_controls(new_session.live_controls());
                                session_id = Some(new_id);
                                session = Some(new_session);
                            }
                            Err(error) => conversation.finish_reconnection(revision, Err(provider_failure_kind(&error))),
                        }
                    }
                    Some(ConversationCommand::OpenResult { request_id }) => {
                        if !conversation.begin_result_preview(request_id) { continue; }
                        let project = workspace.as_path().to_path_buf();
                        let hub = conversation.clone();
                        turns.spawn(async move {
                            let bundle = tokio::task::spawn_blocking(move || result_source::snapshot(&project)).await.unwrap_or(None);
                            let lease = match bundle {
                                Some(bundle) => urdia_ui::ResultPreviewLease::start(bundle).await.ok(),
                                None => None,
                            };
                            hub.finish_result_preview(request_id, lease);
                            (request_id, Ok(()))
                        });
                    }
                    Some(ConversationCommand::Verify { request_id, contract }) => {
                        if !conversation.begin_verification(request_id) { continue; }
                        let (sender, receiver) = mpsc::channel(1);
                        active_cancel = Some((request_id, sender));
                        let project = workspace.as_path().to_path_buf();
                        let hub = conversation.clone();
                        turns.spawn(async move {
                            let report = tokio::task::spawn_blocking(move || verification::run(&project, contract, receiver)).await.unwrap_or(None);
                            hub.finish_verification(request_id, report);
                            (request_id, Ok(()))
                        });
                    }
                    Some(ConversationCommand::CancelVerification { request_id }) => {
                        if let Some((_, sender)) = active_cancel.as_ref().filter(|(id, _)| *id == request_id) { let _ = sender.try_send(()); }
                    }
                    Some(ConversationCommand::Cancel { turn_id }) => {
                        let signalled = active_cancel
                            .as_ref()
                            .filter(|(active_id, _)| *active_id == turn_id)
                            .is_some_and(|(_, sender)| sender.try_send(()).is_ok());
                        if !signalled {
                            let _ = conversation.cancel(turn_id);
                        }
                    }
                    None => break,
                },
                joined = turns.join_next(), if !turns.is_empty() => {
                    let (turn_id, turn_result) = joined
                        .expect("guard guarantees one active turn")
                        .map_err(|error| {
                            UrdiaError::Provider(format!("join conversation turn: {error}"))
                        })?;
                    if active_cancel
                        .as_ref()
                        .is_some_and(|(active_id, _)| *active_id == turn_id)
                    {
                        active_cancel = None;
                    }
                    turn_result?;
                }
            }
        }

        if let Some(session_id) = session_id {
            store.append(Event::new(session_id, EventKind::SessionFinished,
                json!({"status": "completed"}))).await?;
        }
        Ok(())
    }
    .await;

    conversation.stop_result_previews();
    let ui_result = live_ui.stop().await;
    run_result.and(ui_result)
}

async fn start_reconnected_session(
    provider: &dyn urdia_core::AgentProvider,
    request: SessionRequest,
) -> Result<Arc<dyn AgentSession>> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        provider.start_session(request),
    )
    .await
    {
        Ok(result) => result.map(Arc::from),
        Err(_) => Err(UrdiaError::ProviderFailure(
            urdia_core::ProviderFailureKind::Timeout,
        )),
    }
}

struct ConversationUpdateSink {
    conversation: ConversationHub,
    turn_id: Uuid,
    response_limit_reached: Arc<AtomicBool>,
}

impl AgentUpdateSink for ConversationUpdateSink {
    fn publish(&self, update: AgentUpdate) {
        match update {
            update @ (AgentUpdate::ToolActivity(_) | AgentUpdate::ToolActivityLimitReached) => {
                self.conversation
                    .publish_tool_activity(self.turn_id, update);
            }
            AgentUpdate::TextDelta { text } => {
                if matches!(
                    self.conversation.publish_delta(self.turn_id, &text),
                    Err(ConversationUpdateError::ResponseLimit)
                ) {
                    self.response_limit_reached.store(true, Ordering::Release);
                }
            }
        }
    }
}

struct RecommendedTurn {
    profile: Option<ExecutionProfile>,
    provider: Option<Arc<dyn urdia_core::AgentProvider>>,
    workspace: WorkspacePath,
    store: SqliteEventStore,
    receipt_sink: Arc<SqlitePermissionReceiptSink>,
    broker: Option<InteractivePermissionBroker>,
    conversation: ConversationHub,
    turn_id: Uuid,
    text: String,
    persist_content: bool,
    cancel_receiver: mpsc::Receiver<()>,
}

/// Fresh session for one explicitly confirmed profile; primary chat is unchanged.
async fn run_recommended_turn(mut task: RecommendedTurn) -> Result<()> {
    let (Some(profile), Some(provider)) = (&task.profile, &task.provider) else {
        return finish_failed_turn(
            &task.conversation,
            task.turn_id,
            "The confirmed profile is unavailable.",
        );
    };
    let selection = if task
        .conversation
        .snapshot()
        .turns
        .iter()
        .find(|t| t.turn_id == task.turn_id)
        .and_then(|t| t.execution.as_ref())
        .and_then(|c| c.profile.as_ref())
        .is_some_and(|p| p.selection == "manual-next-task")
    {
        "manual-next-task"
    } else {
        "operator-confirmed"
    };
    task.conversation.set_execution(
        task.turn_id,
        ConversationContext {
            provider_id: provider.info().id,
            provider_name: provider.info().name,
            workspace_name: task.workspace.as_path().file_name().map_or_else(
                || "Project".into(),
                |name| name.to_string_lossy().into_owned(),
            ),
            profile: Some(profile.view(selection)),
            startup_controls: None,
        },
    );
    let Some(controls) = profile
        .session_controls
        .clone()
        .filter(|c| c.model.is_some() && (profile.effort.is_none() || c.effort.is_some()))
    else {
        return finish_failed_turn(
            &task.conversation,
            task.turn_id,
            "Exact model and effort controls are required.",
        );
    };
    if !provider.info().availability.is_available() {
        return finish_failed_turn(
            &task.conversation,
            task.turn_id,
            "The confirmed agent is unavailable.",
        );
    }
    let mut request = SessionRequest::new(task.workspace.clone())
        .controls(controls.clone())
        .permission_receipt_sink(task.receipt_sink);
    if let Some(broker) = task.broker {
        request = request.permission_broker(Arc::new(broker));
    }
    let started = tokio::select! {
        biased;
        _ = task.cancel_receiver.recv() => {
            let _ = task.conversation.cancel(task.turn_id);
            return Ok(());
        }
        result = tokio::time::timeout(std::time::Duration::from_secs(45), provider.start_session(request)) => result,
    };
    let session: Arc<dyn AgentSession> = match started {
        Ok(Ok(session)) => Arc::from(session),
        Ok(Err(urdia_core::UrdiaError::ProviderFailure(kind)))
            if kind != urdia_core::ProviderFailureKind::Unknown =>
        {
            return finish_failed_turn(&task.conversation, task.turn_id, kind.message());
        }
        Err(_) => {
            return finish_failed_turn(
                &task.conversation,
                task.turn_id,
                urdia_core::ProviderFailureKind::Timeout.message(),
            );
        }
        Ok(Err(_)) => {
            return finish_failed_turn(
                &task.conversation,
                task.turn_id,
                "Could not start the confirmed agent with its exact settings.",
            );
        }
    };
    let session_id = session.id().clone();
    let reported = session.startup_controls();
    let matched = reported.as_ref().is_some_and(|r| {
        r.model == controls.model && (controls.effort.is_none() || r.effort == controls.effort)
    }) && session
        .live_controls()
        .is_some_and(|r| r.explicit_configuration_matches);
    if !matched {
        let _ = session.cancel().await;
        return finish_failed_turn(
            &task.conversation,
            task.turn_id,
            "The agent did not confirm the requested model and effort. No prompt was sent.",
        );
    }
    let info = provider.info();
    task.conversation.set_execution(
        task.turn_id,
        ConversationContext {
            provider_id: info.id.clone(),
            provider_name: info.name.clone(),
            workspace_name: task.workspace.as_path().file_name().map_or_else(
                || "Project".into(),
                |name| name.to_string_lossy().into_owned(),
            ),
            profile: Some(profile.view(selection)),
            startup_controls: reported,
        },
    );
    task.store
        .append(Event::new(
            session_id.clone(),
            EventKind::SessionStarted,
            provider_event_data(&info, &task.workspace),
        ))
        .await?;
    let result = run_chat_turn(ChatTurn {
        session: session.clone(),
        store: task.store.clone(),
        session_id: session_id.clone(),
        conversation: task.conversation,
        turn_id: task.turn_id,
        text: task.text,
        persist_content: task.persist_content,
        cancel_receiver: task.cancel_receiver,
    })
    .await;
    // Drop the fresh session after the task; never reuse its authorization scope.
    task.store
        .append(Event::new(
            session_id,
            EventKind::SessionFinished,
            json!({"status":"closed"}),
        ))
        .await?;
    result
}

struct ChatTurn {
    session: Arc<dyn AgentSession>,
    store: SqliteEventStore,
    session_id: urdia_core::SessionId,
    conversation: ConversationHub,
    turn_id: Uuid,
    text: String,
    persist_content: bool,
    cancel_receiver: mpsc::Receiver<()>,
}

async fn run_chat_turn(turn: ChatTurn) -> Result<()> {
    let ChatTurn {
        session,
        store,
        session_id,
        conversation,
        turn_id,
        text,
        persist_content,
        mut cancel_receiver,
    } = turn;
    store
        .append(Event::new(
            session_id.clone(),
            EventKind::PromptSubmitted,
            if persist_content {
                json!({"turn_id": turn_id, "text": text})
            } else {
                json!({
                    "turn_id": turn_id,
                    "content_persisted": false,
                    "characters": text.chars().count()
                })
            },
        ))
        .await?;

    let response_limit_reached = Arc::new(AtomicBool::new(false));
    let sink = Arc::new(ConversationUpdateSink {
        conversation: conversation.clone(),
        turn_id,
        response_limit_reached: Arc::clone(&response_limit_reached),
    });
    if conversation_turn_is_cancelling(&conversation, turn_id) {
        let _ = conversation.cancel(turn_id);
        return record_chat_cancellation(&store, session_id).await;
    }

    let prompt = session.prompt_with_updates(Prompt::new(text), sink);
    tokio::pin!(prompt);
    let prompt_polled = Arc::new(AtomicBool::new(false));
    let observed_prompt = {
        let prompt_polled = Arc::clone(&prompt_polled);
        poll_fn(move |context| {
            prompt_polled.store(true, Ordering::Release);
            prompt.as_mut().poll(context)
        })
    };
    tokio::pin!(observed_prompt);
    let result = tokio::select! {
        biased;
        cancellation = cancel_receiver.recv() => {
            if cancellation.is_none() {
                Some(observed_prompt.await)
            } else if prompt_polled.load(Ordering::Acquire) {
                let cancellation_result = session.cancel().await;
                let _ = observed_prompt.await;
                if cancellation_result.is_err() {
                    let _ = conversation.fail(
                        turn_id,
                        "The provider could not cancel this turn.".to_owned(),
                    );
                    return store
                        .append(Event::new(
                            session_id,
                            EventKind::Error,
                            json!({"stage": "cancel", "details_persisted": false}),
                        ))
                        .await;
                }
                None
            } else {
                None
            }
        }
        result = &mut observed_prompt => Some(result),
    };
    let Some(result) = result else {
        let _ = conversation.cancel(turn_id);
        return record_chat_cancellation(&store, session_id).await;
    };
    let cancellation_requested = conversation
        .snapshot()
        .turns
        .iter()
        .find(|turn| turn.turn_id == turn_id)
        .is_some_and(|turn| {
            matches!(
                turn.status,
                ConversationTurnStatus::Cancelling | ConversationTurnStatus::Cancelled
            )
        });

    match result {
        Ok(_) if cancellation_requested => {
            let _ = conversation.cancel(turn_id);
            store
                .append(Event::new(
                    session_id,
                    EventKind::Error,
                    json!({"stage": "prompt", "cancelled": true, "details_persisted": false}),
                ))
                .await
        }
        Ok(result) => {
            store
                .append(Event::new(
                    session_id,
                    EventKind::AgentText,
                    if persist_content {
                        json!({
                            "turn_id": turn_id,
                            "text": result.text,
                            "stop_reason": result.stop_reason
                        })
                    } else {
                        json!({
                            "turn_id": turn_id,
                            "content_persisted": false,
                            "characters": result.text.chars().count(),
                            "stop_reason": result.stop_reason
                        })
                    },
                ))
                .await?;
            if response_limit_reached.load(Ordering::Acquire) {
                finish_failed_turn(
                    &conversation,
                    turn_id,
                    "The response exceeded the local display limit.",
                )
            } else {
                finish_completed_turn(&conversation, turn_id)
            }
        }
        Err(_) if cancellation_requested => {
            let _ = conversation.cancel(turn_id);
            store
                .append(Event::new(
                    session_id,
                    EventKind::Error,
                    json!({"stage": "prompt", "cancelled": true, "details_persisted": false}),
                ))
                .await
        }
        Err(error) => {
            let kind = provider_failure_kind(&error);
            let _ = conversation.fail(turn_id, kind.message().to_owned());
            store
                .append(Event::new(
                    session_id,
                    EventKind::Error,
                    json!({"stage": "prompt", "turn_id": turn_id, "failure_kind":kind, "details_persisted": false}),
                ))
                .await
        }
    }
}

fn provider_failure_kind(error: &UrdiaError) -> urdia_core::ProviderFailureKind {
    match error {
        UrdiaError::ProviderFailure(kind) => *kind,
        _ => urdia_core::ProviderFailureKind::Unknown,
    }
}

fn conversation_turn_is_cancelling(conversation: &ConversationHub, turn_id: Uuid) -> bool {
    conversation
        .snapshot()
        .turns
        .iter()
        .find(|turn| turn.turn_id == turn_id)
        .is_some_and(|turn| {
            matches!(
                turn.status,
                ConversationTurnStatus::Cancelling | ConversationTurnStatus::Cancelled
            )
        })
}

async fn record_chat_cancellation(
    store: &SqliteEventStore,
    session_id: urdia_core::SessionId,
) -> Result<()> {
    store
        .append(Event::new(
            session_id,
            EventKind::Error,
            json!({"stage": "prompt", "cancelled": true, "details_persisted": false}),
        ))
        .await
}

fn finish_completed_turn(conversation: &ConversationHub, turn_id: Uuid) -> Result<()> {
    match conversation.complete(turn_id) {
        Ok(()) => Ok(()),
        Err(ConversationUpdateError::InvalidState {
            status: ConversationTurnStatus::Cancelling | ConversationTurnStatus::Cancelled,
            ..
        }) => {
            let _ = conversation.cancel(turn_id);
            Ok(())
        }
        Err(error) => Err(conversation_state_error(error)),
    }
}

fn begin_conversation_turn(conversation: &ConversationHub, turn_id: Uuid) -> Result<bool> {
    match conversation.mark_running(turn_id) {
        Ok(()) => Ok(true),
        Err(ConversationUpdateError::InvalidState {
            status: ConversationTurnStatus::Cancelling | ConversationTurnStatus::Cancelled,
            ..
        }) => {
            let _ = conversation.cancel(turn_id);
            Ok(false)
        }
        Err(error) => Err(conversation_state_error(error)),
    }
}

fn finish_failed_turn(conversation: &ConversationHub, turn_id: Uuid, message: &str) -> Result<()> {
    match conversation.fail(turn_id, message.to_owned()) {
        Ok(()) => Ok(()),
        Err(ConversationUpdateError::InvalidState {
            status: ConversationTurnStatus::Cancelling | ConversationTurnStatus::Cancelled,
            ..
        }) => {
            let _ = conversation.cancel(turn_id);
            Ok(())
        }
        Err(error) => Err(conversation_state_error(error)),
    }
}

fn conversation_state_error(error: ConversationUpdateError) -> UrdiaError {
    UrdiaError::Ui(format!("conversation state transition failed: {error}"))
}

struct ActiveLiveUi {
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<()>>,
}

impl ActiveLiveUi {
    fn start(server: LiveUiServer) -> Self {
        let (shutdown, receiver) = oneshot::channel();
        let task = tokio::spawn(server.serve_until(async move {
            let _ = receiver.await;
        }));
        Self { shutdown, task }
    }

    async fn stop(self) -> Result<()> {
        let _ = self.shutdown.send(());
        self.task
            .await
            .map_err(|error| UrdiaError::Ui(format!("join live UI task: {error}")))?
    }
}

fn provider_event_data(info: &ProviderInfo, workspace: &WorkspacePath) -> serde_json::Value {
    json!({
        "provider": info.id,
        "provider_name": info.name,
        "protocols": info.protocols,
        "authentication": info.authentication,
        "inference": info.inference,
        "workspace": workspace.display()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use urdia_core::{
        AuthenticationState, InferenceLocation, PermissionAuthorizationSource,
        PermissionOperationKind, PermissionReceiptOutcome, PromptResult, ProviderAvailability,
        SessionId,
    };
    use uuid::Uuid;

    struct CountingSession {
        id: SessionId,
        prompts: Arc<AtomicUsize>,
        cancellations: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentSession for CountingSession {
        fn id(&self) -> &SessionId {
            &self.id
        }

        async fn prompt(&self, _prompt: Prompt) -> Result<PromptResult> {
            self.prompts.fetch_add(1, Ordering::AcqRel);
            Ok(PromptResult {
                text: "unexpected".to_owned(),
                stop_reason: None,
            })
        }

        async fn cancel(&self) -> Result<()> {
            self.cancellations.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }

    struct DispatchProvider {
        starts: Arc<AtomicUsize>,
        prompts: Arc<AtomicUsize>,
        matched: bool,
    }
    #[async_trait]
    impl urdia_core::AgentProvider for DispatchProvider {
        fn info(&self) -> ProviderInfo {
            let mut info =
                ProviderInfo::new("dedicated", "Dedicated test agent", vec!["acp-v1".into()]);
            info.availability = ProviderAvailability::Available;
            info
        }
        async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>> {
            assert_eq!(request.controls.model.as_deref(), Some("exact-model"));
            assert_eq!(request.controls.effort.as_deref(), Some("low"));
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(DispatchSession {
                id: SessionId::new(),
                prompts: self.prompts.clone(),
                matched: self.matched,
            }))
        }
    }
    struct DispatchSession {
        id: SessionId,
        prompts: Arc<AtomicUsize>,
        matched: bool,
    }
    #[async_trait]
    impl AgentSession for DispatchSession {
        fn id(&self) -> &SessionId {
            &self.id
        }
        fn startup_controls(&self) -> Option<urdia_core::SessionControlState> {
            Some(urdia_core::SessionControlState {
                model: Some(
                    if self.matched {
                        "exact-model"
                    } else {
                        "wrong-model"
                    }
                    .into(),
                ),
                effort: Some("low".into()),
            })
        }
        fn live_controls(&self) -> Option<urdia_core::LiveSessionControls> {
            Some(urdia_core::LiveSessionControls {
                revision: 0,
                current: self.startup_controls(),
                changed_since_startup: false,
                explicit_configuration_matches: self.matched,
            })
        }
        async fn prompt(&self, _: Prompt) -> Result<PromptResult> {
            self.prompts.fetch_add(1, Ordering::SeqCst);
            Ok(PromptResult {
                text: "20".into(),
                stop_reason: None,
            })
        }
        async fn cancel(&self) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn execution_eligibility_requires_host_scope_and_exact_pins_not_labels() {
        let starts = Arc::new(AtomicUsize::new(0));
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(DispatchProvider {
                starts: starts.clone(),
                prompts: Arc::new(AtomicUsize::new(0)),
                matched: true,
            })
            .unwrap();
        for (controls, effort, eligible) in [
            (None, None, false),
            (
                Some(urdia_core::SessionControls {
                    model: None,
                    effort: Some("low".into()),
                }),
                Some("Low".into()),
                false,
            ),
            (
                Some(urdia_core::SessionControls {
                    model: Some("exact-model".into()),
                    effort: None,
                }),
                Some("Low".into()),
                false,
            ),
            (
                Some(urdia_core::SessionControls {
                    model: Some("exact-model".into()),
                    effort: Some("low".into()),
                }),
                Some("Low".into()),
                true,
            ),
        ] {
            let catalog = ProfileCatalog::new(
                vec![ExecutionProfile {
                    id: "target".into(),
                    name: "Target".into(),
                    provider: "dedicated".into(),
                    agent: "Agent".into(),
                    inference_provider: "Unknown".into(),
                    model: "Label".into(),
                    effort,
                    session_controls: controls,
                }],
                None,
                &registry,
            )
            .unwrap();
            let mut advice = BrowserAdvisory::for_profiles(&registry, &catalog).unwrap();
            let result =
                urdia_ui::AdvisoryProvider::recommend(&advice, "deterministic", "task".into())
                    .await
                    .unwrap();
            assert!(urdia_ui::AdvisoryProvider::execution_target(&advice, &result).is_none());
            assert!(!urdia_ui::AdvisoryProvider::automatic_execution_available(
                &advice
            ));
            advice.execution_scope = Some(("Example".into(), "one-shot approval".into()));
            assert_eq!(
                urdia_ui::AdvisoryProvider::execution_target(&advice, &result).is_some(),
                eligible
            );
            assert_eq!(
                urdia_ui::AdvisoryProvider::automatic_execution_available(&advice),
                eligible
            );
            let mut rejected = result;
            rejected.fallback_reason = Some("invalid_decision".into());
            assert!(urdia_ui::AdvisoryProvider::execution_target(&advice, &rejected).is_none());
        }
        assert_eq!(starts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn confirmed_dispatch_blocks_pin_mismatch_and_prestart_cancellation() {
        for (matched, cancelled, expected_starts, expected_prompts, status) in [
            (false, false, 1, 0, ConversationTurnStatus::Failed),
            (true, true, 0, 0, ConversationTurnStatus::Cancelled),
            (true, false, 1, 1, ConversationTurnStatus::Completed),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let workspace = WorkspacePath::new(dir.path()).unwrap();
            let store = SqliteEventStore::in_memory().unwrap();
            let starts = Arc::new(AtomicUsize::new(0));
            let prompts = Arc::new(AtomicUsize::new(0));
            let (conversation, mut commands) = ConversationHub::new();
            let turn_id = Uuid::new_v4();
            conversation
                .submit_recommended(turn_id, "sum five numbers".into(), "target".into())
                .unwrap();
            commands.try_recv().unwrap();
            conversation.mark_running(turn_id).unwrap();
            let (sender, receiver) = mpsc::channel(1);
            if cancelled {
                sender.try_send(()).unwrap();
            }
            run_recommended_turn(RecommendedTurn {
                profile: Some(ExecutionProfile {
                    id: "target".into(),
                    name: "Target".into(),
                    provider: "dedicated".into(),
                    agent: "Test".into(),
                    inference_provider: "Unknown".into(),
                    model: "Display label".into(),
                    effort: Some("Low".into()),
                    session_controls: Some(urdia_core::SessionControls {
                        model: Some("exact-model".into()),
                        effort: Some("low".into()),
                    }),
                }),
                provider: Some(Arc::new(DispatchProvider {
                    starts: starts.clone(),
                    prompts: prompts.clone(),
                    matched,
                })),
                workspace,
                store: store.clone(),
                receipt_sink: Arc::new(SqlitePermissionReceiptSink { store }),
                broker: None,
                conversation: conversation.clone(),
                turn_id,
                text: "sum five numbers".into(),
                persist_content: false,
                cancel_receiver: receiver,
            })
            .await
            .unwrap();
            assert_eq!(starts.load(Ordering::SeqCst), expected_starts);
            assert_eq!(prompts.load(Ordering::SeqCst), expected_prompts);
            assert_eq!(conversation.snapshot().turns[0].status, status);
            assert_eq!(
                conversation.snapshot().turns[0]
                    .execution
                    .as_ref()
                    .unwrap()
                    .profile
                    .as_ref()
                    .unwrap()
                    .selection,
                "operator-confirmed"
            );
        }
    }

    #[tokio::test]
    async fn profile_advice_filters_unavailable_launches_and_maps_typed_results() {
        let (_dir, path) = write_provider_config(
            r#"
            default_profile = "backend"
            [[provider]]
            id = "backend-launch"
            name = "Backend launch"
            command = "/usr/bin/true"
            args = ["SECRET_ARG_SENTINEL"]
            [[provider]]
            id = "missing-launch"
            name = "Missing launch"
            command = "/missing/agent"
            [[profile]]
            id = "backend"
            name = "Configured backend"
            provider = "backend-launch"
            agent = "Configured ACP agent"
            inference_provider = "Configured service"
            model = "DECLARED_MODEL_SENTINEL"
            effort = "DECLARED_EFFORT_SENTINEL"
            [[profile]]
            id = "missing"
            name = "Missing profile"
            provider = "missing-launch"
            agent = "Configured ACP agent"
            inference_provider = "Configured service"
            model = "Unavailable model"
        "#,
        );
        let (registry, catalog) = build_configuration(Some(&path)).unwrap();
        let mut advisory = BrowserAdvisory::for_profiles(&registry, &catalog).unwrap();
        assert_eq!(advisory.candidates.len(), 1);
        let state = serde_json::to_string(&advisory.state).unwrap();
        assert!(state.contains("DECLARED_MODEL_SENTINEL"));
        assert!(state.contains("DECLARED_EFFORT_SENTINEL"));
        assert!(!state.contains("SECRET_ARG_SENTINEL"));
        assert!(!state.contains("/usr/bin/true"));
        assert!(!state.contains("Unavailable model"));
        assert_eq!(
            advisory.state["execution_profiles"]["route_000"]["model_effort_verified"],
            false
        );
        assert!(advisory.state["execution_profiles"]["route_000"]["measured_cost"].is_null());
        use urdia_ui::AdvisoryProvider;
        assert_eq!(advisory.candidate_kind(), "profile");
        let local = BrowserAdvisory::prepared_profiles(&registry, &catalog, false).unwrap();
        assert_eq!(local.candidate_kind(), "profile");
        assert!(local.engines.is_empty());
        assert!(local.jev.resolver().is_none());
        assert!(local.laya.resolver().is_none());
        assert_eq!(local.profiles.len(), advisory.profiles.len());
        struct Capture(Arc<std::sync::Mutex<Option<Value>>>);
        #[async_trait]
        impl urdia_core::DecisionProvider for Capture {
            fn id(&self) -> &str {
                "capturing"
            }
            async fn decide(
                &self,
                input: urdia_core::DecisionInput,
            ) -> Result<urdia_core::Decision> {
                *self.0.lock().unwrap() = Some(input.state.clone());
                FirstCandidateDecisionProvider::new("capturing")
                    .decide(input)
                    .await
            }
        }
        let captured = Arc::new(std::sync::Mutex::new(None));
        advisory.engines.insert(
            "deterministic".to_owned(),
            Some(RouteResolver::new(Arc::new(Capture(captured.clone())))),
        );
        let result = advisory
            .recommend("deterministic", "A small calculator".to_owned())
            .await
            .unwrap();
        assert_eq!(result.selected_provider.as_deref(), Some("backend-launch"));
        assert_eq!(result.selected_profile.unwrap().id, "backend");
        assert_eq!(
            result.probabilities.keys().collect::<Vec<_>>(),
            vec!["backend"]
        );
        assert!(result.fallback_reason.is_none());
        assert_eq!(
            captured.lock().unwrap().as_ref().unwrap()["context"]["execution_profiles"]["route_000"]
                ["declared_configuration"]["model"],
            "DECLARED_MODEL_SENTINEL"
        );
        assert!(BrowserAdvisory::for_profiles(&registry, &ProfileCatalog::default()).is_err());
        let legacy = BrowserAdvisory::new(&registry).unwrap();
        assert_eq!(legacy.candidate_kind(), "provider");
        assert_eq!(legacy.state, json!({}));
        let database = _dir.path().join("must-not-exist.sqlite");
        let failed = chat_provider(
            &registry,
            "backend-launch",
            None,
            urdia_core::SessionControls::default(),
            _dir.path().to_owned(),
            database.clone(),
            false,
            false,
            true,
            false,
            Some(&ProfileCatalog::default()),
            false,
            "127.0.0.1:0".parse().unwrap(),
            None,
            None,
            &ProfileCatalog::default(),
        )
        .await;
        assert!(failed.is_err());
        assert!(
            !database.exists(),
            "profile preparation must fail before durable state"
        );
    }

    #[test]
    fn registration_stages_setup_only_after_persistence_and_preserves_advice() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let registry = build_registry(None).unwrap();
        // Include an available offline launch so the advisory constructor requires no real CLI.
        let mut registry = registry;
        registry
            .register_owned(AcpProvider::with_metadata(
                "offline",
                "Offline",
                AcpCommand::new("/usr/bin/true"),
            ))
            .unwrap();
        let mut advice = BrowserAdvisory::local_only(&registry).unwrap();
        let before = advice.candidates.len();
        let editor = Arc::new(
            profile_store::ProfileStore::new(
                &registry,
                &ProfileCatalog::default(),
                Some(&dir.path().join("profiles.toml")),
            )
            .unwrap(),
        );
        let mut agents =
            agent_store::AgentStore::load(Some(&dir.path().join("agents.toml"))).unwrap();
        agents.bind_registry(&registry);
        advice.editor = Some(editor.clone());
        advice.agent_editor = Some(Arc::new(agents));
        let request = || urdia_ui::RegisterAgentRequest {
            revision: 0,
            confirm_launch: true,
            agent: urdia_ui::AgentDraft {
                id: "pending".into(),
                name: "Pending".into(),
                command: "/usr/bin/true".into(),
                args: vec![],
            },
        };
        urdia_ui::AdvisoryProvider::register_agent(&advice, request()).unwrap();
        assert!(
            editor
                .snapshot()
                .providers
                .iter()
                .any(|p| p.id == "pending" && p.pending_restart)
        );
        assert!(registry.resolve("pending").is_err());
        assert_eq!(advice.candidates.len(), before);
        assert!(advice.execution_providers.is_empty());
        let revision = editor.snapshot().revision;
        assert!(urdia_ui::AdvisoryProvider::register_agent(&advice, request()).is_err());
        assert_eq!(editor.snapshot().revision, revision);
    }

    #[test]
    fn next_launch_agents_are_registered_before_profile_validation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"[[profile]]
id = "backend"
name = "Backend"
provider = "new-agent"
agent = "External"
inference_provider = "Unknown"
model = "Operator configured"
"#,
        )
        .unwrap();
        let definitions = vec![urdia_ui::AgentDraft {
            id: "new-agent".into(),
            name: "New agent".into(),
            command: "missing-agent".into(),
            args: vec!["--acp".into()],
        }];
        let (registry, catalog) =
            build_configuration_with_agents(Some(&path), definitions.clone()).unwrap();
        assert!(registry.resolve("new-agent").is_ok());
        assert_eq!(catalog.profiles["backend"].provider, "new-agent");
        std::fs::write(
            &path,
            "[[provider]]\nid = 'new-agent'\nname = 'Duplicate'\ncommand = 'anything'\n",
        )
        .unwrap();
        assert!(build_configuration_with_agents(Some(&path), definitions).is_err());
    }
    #[test]
    fn settings_destinations_refuse_absent_aliases_and_hardlinks() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("agents.toml");
        let alias = dir.path().join(".").join("agents.toml");
        assert!(!distinct_settings(&a, &alias).unwrap());
        assert!(distinct_settings(&a, &dir.path().join("profiles.toml")).unwrap());
        std::fs::write(&a, b"provider = []").unwrap();
        let b = dir.path().join("other.toml");
        std::fs::hard_link(&a, &b).unwrap();
        assert!(!distinct_settings(&a, &b).unwrap());
    }

    #[test]
    fn default_chat_local_configuration_does_not_enable_host_engines() {
        use urdia_ui::AdvisoryProvider;
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(AcpProvider::with_metadata(
                "fixture",
                "Fixture",
                AcpCommand::new(std::env::current_exe().unwrap()),
            ))
            .unwrap();
        let local = BrowserAdvisory::local_only(&registry).unwrap();
        assert!(local.engines.is_empty());
        let engines = local.engines();
        assert_eq!(engines.len(), 2);
        assert_eq!(engines[0].id, "jev");
        assert!(!engines[0].configured);
        assert!(engines[0].remote);
        assert_eq!(local.jev_connection().unwrap().status, "disconnected");
        assert_eq!(engines[1].id, "laya");
        assert!(!engines[1].configured);
        assert!(!engines[1].remote);
        assert_eq!(local.laya_connection().unwrap().status, "disconnected");
    }

    #[test]
    fn profile_advice_requires_advisory_opt_in() {
        assert!(
            Cli::try_parse_from(["urdia", "chat", "--provider", "qwen", "--advisory-profiles"])
                .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "urdia",
                "chat",
                "--provider",
                "qwen",
                "--advisory-ui",
                "--advisory-profiles"
            ])
            .is_ok()
        );
    }

    #[test]
    fn profile_cli_selection_is_exclusive_and_default_can_be_resolved_later() {
        assert!(
            Cli::try_parse_from([
                "urdia",
                "chat",
                "--provider",
                "qwen",
                "--profile",
                "my-profile"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "urdia",
                "run",
                "--profile",
                "my-profile",
                "--prompt",
                "hello"
            ])
            .is_ok()
        );
        assert!(Cli::try_parse_from(["urdia", "chat"]).is_ok());
    }

    #[test]
    fn config_profiles_reference_exact_registered_launch_definitions() {
        let (_dir, path) = write_provider_config(
            r#"
            default_profile = "backend"
            [[provider]]
            id = "backend-launch"
            name = "Backend ACP"
            command = "/missing/backend-agent"
            args = ["--acp", "--operator-tested-option"]
            [[profile]]
            id = "backend"
            name = "Backend profile"
            provider = "backend-launch"
            agent = "External ACP agent"
            inference_provider = "configured-service"
            model = "configured-model"
            effort = "configured-effort"
        "#,
        );
        let (registry, catalog) = build_configuration(Some(&path)).unwrap();
        assert_eq!(catalog.select(None, None).unwrap(), "backend-launch");
        assert_eq!(
            catalog.profiles["backend"].effort.as_deref(),
            Some("configured-effort")
        );
        assert!(registry.resolve_available("backend-launch").is_err());
    }

    #[test]
    fn parses_external_acp_provider_as_argv() {
        let config: UrdiaConfig = toml::from_str(
            r#"
                [[provider]]
                id = "custom-agent"
                name = "Custom Agent"
                command = "/opt/custom-agent"
                args = ["--acp", "--profile", "safe"]
            "#,
        )
        .expect("valid config should parse");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].command, Path::new("/opt/custom-agent"));
        assert_eq!(config.providers[0].args, ["--acp", "--profile", "safe"]);
    }

    fn write_provider_config(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("providers.toml");
        std::fs::write(&path, contents).expect("provider config should write");
        (directory, path)
    }

    #[test]
    fn external_config_registers_conservative_provider_metadata() {
        let (_directory, path) = write_provider_config(
            r#"
                [[provider]]
                id = "guide-agent"
                name = "Guide Agent"
                command = "/opt/guide/agent"
                args = ["--acp"]
            "#,
        );

        let registry = build_registry(Some(&path)).expect("external config should register");
        assert_eq!(registry.len(), 2);
        let ids: Vec<String> = registry.list().into_iter().map(|info| info.id).collect();
        assert_eq!(ids, ["guide-agent", "qwen"]);

        let info = registry
            .resolve("guide-agent")
            .expect("external provider should resolve without being available")
            .info();
        assert_eq!(info.name, "Guide Agent");
        assert_eq!(info.protocols, ["acp-v1", "stdio"]);
        assert_eq!(info.capabilities, ["sessions", "prompt", "cancel"]);
        assert_eq!(info.authentication, AuthenticationState::Unknown);
        assert_eq!(info.inference, InferenceLocation::Unknown);
        assert_eq!(info.availability, ProviderAvailability::Unavailable);
        assert_eq!(
            info.availability_detail.as_deref(),
            Some("executable not found: /opt/guide/agent")
        );

        match registry.resolve_available("guide-agent") {
            Ok(_) => panic!("an unavailable provider must not resolve for a session"),
            Err(error) => assert!(format!("{error}").contains("executable not found")),
        }
    }

    #[test]
    fn missing_config_file_is_reported_before_any_provider_resolution() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let missing = directory.path().join("absent.toml");
        let error = build_registry(Some(&missing)).expect_err("a missing config must be reported");
        let message = format!("{error}");
        assert!(message.contains("read provider config"), "{message}");
        assert!(message.contains("absent.toml"), "{message}");
    }

    #[test]
    fn external_config_rejects_invalid_and_duplicate_provider_ids() {
        let (_invalid, invalid_path) = write_provider_config(
            r#"
                [[provider]]
                id = "Bad_ID"
                name = "Bad ID"
                command = "/opt/bad-agent"
            "#,
        );
        let error = build_registry(Some(&invalid_path)).expect_err("invalid IDs must be rejected");
        assert!(format!("{error}").contains("expected lowercase ASCII kebab case"));

        let (_duplicate, duplicate_path) = write_provider_config(
            r#"
                [[provider]]
                id = "qwen"
                name = "Impersonating Qwen"
                command = "/opt/other-agent"
            "#,
        );
        let error =
            build_registry(Some(&duplicate_path)).expect_err("duplicate IDs must be rejected");
        assert!(format!("{error}").contains("duplicate provider ID `qwen`"));
    }

    #[test]
    fn custom_ui_bind_requires_explicit_permission_ui_opt_in() {
        let rejected = Cli::try_parse_from([
            "urdia",
            "run",
            "--provider",
            "qwen",
            "--prompt",
            "test",
            "--ui-bind",
            "127.0.0.1:0",
        ]);
        assert!(rejected.is_err());

        let accepted = Cli::try_parse_from([
            "urdia",
            "run",
            "--provider",
            "qwen",
            "--prompt",
            "test",
            "--permissions-ui",
            "--ui-bind",
            "127.0.0.1:0",
        ])
        .expect("explicit permission UI opt-in should parse");
        let Command::Run {
            permissions_ui,
            ui_bind,
            ..
        } = accepted.command
        else {
            panic!("run command should parse");
        };
        assert!(permissions_ui);
        assert_eq!(ui_bind, Some("127.0.0.1:0".parse().unwrap()));
    }

    #[test]
    fn chat_defaults_to_an_ephemeral_loopback_ui() {
        let parsed = Cli::try_parse_from(["urdia", "chat", "--provider", "qwen"])
            .expect("chat command should parse");
        let Command::Chat {
            provider,
            bind,
            permissions_ui,
            advisory_ui,
            verification_ui,
            trusted_session,
            persist_content,
            ..
        } = parsed.command
        else {
            panic!("chat command should parse");
        };
        assert_eq!(provider.as_deref(), Some("qwen"));
        assert_eq!(bind, "127.0.0.1:0".parse().unwrap());
        assert!(!permissions_ui);
        assert!(!advisory_ui);
        assert!(!verification_ui);
        assert!(!trusted_session);
        assert!(!persist_content);
    }

    #[test]
    fn trusted_session_requires_the_authenticated_permission_ui() {
        let rejected =
            Cli::try_parse_from(["urdia", "chat", "--provider", "qwen", "--trusted-session"]);
        assert!(rejected.is_err());

        let accepted = Cli::try_parse_from([
            "urdia",
            "chat",
            "--provider",
            "qwen",
            "--permissions-ui",
            "--trusted-session",
        ])
        .expect("trusted session should require and accept explicit UI opt-in");
        let Command::Chat {
            permissions_ui,
            trusted_session,
            bind,
            ..
        } = accepted.command
        else {
            panic!("chat command should parse");
        };
        assert!(permissions_ui);
        assert!(trusted_session);
        assert!(bind.ip().is_loopback());
    }

    #[test]
    fn routing_is_advisory_and_offline_by_default() {
        let parsed = Cli::try_parse_from([
            "urdia",
            "route",
            "--objective",
            "Review this workspace",
            "--candidate",
            "qwen",
        ])
        .expect("route command should parse");
        let Command::Route {
            router,
            candidates,
            state,
            min_confidence,
            ..
        } = parsed.command
        else {
            panic!("route command should parse");
        };
        assert_eq!(router, RouterKind::Deterministic);
        assert_eq!(candidates, ["qwen"]);
        assert!(state.is_none());
        assert!(min_confidence.is_none());

        let evaluation = Cli::try_parse_from(["urdia", "eval-routing"])
            .expect("evaluation command should parse");
        let Command::EvalRouting {
            fixture,
            router,
            live,
            split,
            repeats,
            ..
        } = evaluation.command
        else {
            panic!("evaluation command should parse");
        };
        assert_eq!(fixture, Path::new("evaluations/routing/v2.json"));
        assert_eq!(router, RouterKind::Deterministic);
        assert!(!live);
        assert_eq!(split, EvaluationSplit::All);
        assert_eq!(repeats, 1);

        let laya_route = Cli::try_parse_from([
            "urdia",
            "route",
            "--router",
            "laya",
            "--objective",
            "Review this workspace",
            "--candidate",
            "qwen",
        ])
        .expect("Laya route command should parse");
        let Command::Route { router, .. } = laya_route.command else {
            panic!("Laya route command should parse");
        };
        assert_eq!(router, RouterKind::Laya);

        let laya_evaluation = Cli::try_parse_from([
            "urdia",
            "eval-routing",
            "--router",
            "laya",
            "--fixture",
            "evaluations/routing/v2-laya.json",
        ])
        .expect("Laya evaluation command should parse");
        let Command::EvalRouting { router, live, .. } = laya_evaluation.command else {
            panic!("Laya evaluation command should parse");
        };
        assert_eq!(router, RouterKind::Laya);
        assert!(!live);

        let typellm_route = Cli::try_parse_from([
            "urdia",
            "route",
            "--router",
            "typellm",
            "--objective",
            "Review this workspace",
            "--candidate",
            "qwen",
        ])
        .expect("TypeLLM route command should parse");
        let Command::Route { router, .. } = typellm_route.command else {
            panic!("TypeLLM route command should parse");
        };
        assert_eq!(router, RouterKind::Typellm);
    }

    #[test]
    fn bounded_json_rejects_oversized_input_before_parsing() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("state.json");
        std::fs::write(&path, br#"{"too_large":true}"#).expect("fixture should write");
        let error =
            read_bounded_json(&path, 8, "test input").expect_err("oversized input should fail");
        assert!(format!("{error}").contains("size limit"));
    }

    #[test]
    fn queued_chat_cancellation_is_a_normal_terminal_outcome() {
        let (conversation, _commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        conversation
            .submit_prompt(turn_id, "cancel before start".to_owned())
            .expect("prompt should queue");
        conversation
            .request_cancel(turn_id)
            .expect("cancellation should queue");

        assert!(!begin_conversation_turn(&conversation, turn_id).expect("normal cancellation"));
        assert_eq!(
            conversation.snapshot().turns[0].status,
            ConversationTurnStatus::Cancelled
        );
    }

    #[test]
    fn completion_racing_with_cancellation_does_not_fail_chat() {
        let (conversation, _commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        conversation
            .submit_prompt(turn_id, "finish race".to_owned())
            .expect("prompt should queue");
        conversation
            .mark_running(turn_id)
            .expect("prompt should run");
        conversation
            .request_cancel(turn_id)
            .expect("cancellation should queue");

        finish_completed_turn(&conversation, turn_id).expect("cancellation should stay normal");
        assert_eq!(
            conversation.snapshot().turns[0].status,
            ConversationTurnStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn cancellation_ready_before_dispatch_never_starts_provider_work() {
        let (conversation, _commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        conversation
            .submit_prompt(turn_id, "do not dispatch".to_owned())
            .expect("prompt should queue");
        conversation
            .mark_running(turn_id)
            .expect("turn should enter host ownership");
        let prompts = Arc::new(AtomicUsize::new(0));
        let cancellations = Arc::new(AtomicUsize::new(0));
        let session: Arc<dyn AgentSession> = Arc::new(CountingSession {
            id: SessionId::new(),
            prompts: Arc::clone(&prompts),
            cancellations: Arc::clone(&cancellations),
        });
        let store = SqliteEventStore::in_memory().expect("in-memory store");
        let session_id = session.id().clone();
        let (cancel_sender, cancel_receiver) = mpsc::channel(1);
        cancel_sender
            .try_send(())
            .expect("cancellation should be ready before dispatch");

        run_chat_turn(ChatTurn {
            session,
            store,
            session_id,
            conversation: conversation.clone(),
            turn_id,
            text: "do not dispatch".to_owned(),
            persist_content: false,
            cancel_receiver,
        })
        .await
        .expect("pre-dispatch cancellation should be normal");

        assert_eq!(prompts.load(Ordering::Acquire), 0);
        assert_eq!(cancellations.load(Ordering::Acquire), 0);
        assert_eq!(
            conversation.snapshot().turns[0].status,
            ConversationTurnStatus::Cancelled
        );
    }

    struct FailingDiagnosticSession {
        id: SessionId,
        prompts: Arc<AtomicUsize>,
        kind: Option<urdia_core::ProviderFailureKind>,
    }
    #[async_trait]
    impl AgentSession for FailingDiagnosticSession {
        fn id(&self) -> &SessionId {
            &self.id
        }
        async fn prompt(&self, _prompt: Prompt) -> Result<PromptResult> {
            self.prompts.fetch_add(1, Ordering::SeqCst);
            Err(self.kind.map_or_else(
                || UrdiaError::Provider("PRIVATE_FAILURE_SENTINEL".into()),
                UrdiaError::ProviderFailure,
            ))
        }
        async fn cancel(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn chat_provider_failures_publish_only_static_message_and_category_without_retry() {
        for kind in [
            Some(urdia_core::ProviderFailureKind::Authentication),
            Some(urdia_core::ProviderFailureKind::RateOrQuota),
            None,
        ] {
            let (conversation, _commands) = ConversationHub::new();
            let turn_id = Uuid::new_v4();
            conversation
                .submit_prompt(turn_id, "synthetic fixture".into())
                .unwrap();
            conversation.mark_running(turn_id).unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let session_id = SessionId::new();
            let session: Arc<dyn AgentSession> = Arc::new(FailingDiagnosticSession {
                id: session_id.clone(),
                prompts: calls.clone(),
                kind,
            });
            let store = SqliteEventStore::in_memory().unwrap();
            let (_cancel_sender, cancel_receiver) = mpsc::channel(1);
            run_chat_turn(ChatTurn {
                session,
                store: store.clone(),
                session_id: session_id.clone(),
                conversation: conversation.clone(),
                turn_id,
                text: "synthetic fixture".into(),
                persist_content: false,
                cancel_receiver,
            })
            .await
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            let snapshot = conversation.snapshot();
            assert_eq!(snapshot.turns.len(), 1);
            assert_eq!(snapshot.turns[0].status, ConversationTurnStatus::Failed);
            let expected = kind.unwrap_or(urdia_core::ProviderFailureKind::Unknown);
            assert_eq!(snapshot.turns[0].error.as_deref(), Some(expected.message()));
            assert!(snapshot.active_turn_id.is_none());
            let events = store.list(&session_id).await.unwrap();
            let errors: Vec<_> = events
                .iter()
                .filter(|event| event.event.kind == EventKind::Error)
                .collect();
            assert_eq!(errors.len(), 1);
            assert_eq!(
                errors[0].event.data["failure_kind"],
                serde_json::to_value(expected).unwrap()
            );
            assert_eq!(errors[0].event.data["details_persisted"], false);
            assert!(
                !serde_json::to_string(
                    &events
                        .iter()
                        .map(|event| &event.event.data)
                        .collect::<Vec<_>>()
                )
                .unwrap()
                .contains("PRIVATE_FAILURE_SENTINEL")
            );
        }
    }

    #[tokio::test]
    async fn permission_sink_persists_only_redacted_receipt_shape() {
        let store = SqliteEventStore::in_memory().expect("in-memory store");
        let sink = SqlitePermissionReceiptSink {
            store: store.clone(),
        };
        let session_id = SessionId::new();
        sink.record(PermissionReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            session_id: session_id.clone(),
            revision: 2,
            provider_session_fingerprint: "provider-hash".to_owned(),
            workspace_fingerprint: "workspace-hash".to_owned(),
            operation_fingerprint: "operation-hash".to_owned(),
            tool_call_fingerprint: "tool-hash".to_owned(),
            kind: PermissionOperationKind::Execute,
            arguments_present: true,
            location_count: 1,
            authorization: PermissionAuthorizationSource::OneShot,
            policy_id_fingerprint: None,
            policy_revision: None,
            outcome: PermissionReceiptOutcome::RejectedOnce,
            cancellation_reason: None,
        })
        .await
        .expect("receipt should persist");

        let events = store.list(&session_id).await.expect("events should load");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.kind, EventKind::Permission);
        let serialized = serde_json::to_string(&events[0].event.data).expect("event JSON");
        assert!(!serialized.contains("SECRET_SENTINEL"));
        assert_eq!(events[0].event.data["revision"], 2);
        assert_eq!(events[0].event.data["outcome"], "rejected_once");
    }

    #[tokio::test]
    async fn timeline_reads_existing_database_and_omits_retained_content() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("events.sqlite");
        let output = directory.path().join("view/timeline.html");
        let store = SqliteEventStore::open(&database).expect("database should open");
        let session_id = SessionId::new();
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::SessionStarted,
                json!({
                    "provider": "qwen",
                    "provider_name": "Qwen Code",
                    "workspace": "/private/SECRET_WORKSPACE"
                }),
            ))
            .await
            .expect("session should append");
        store
            .append(Event::new(
                session_id,
                EventKind::PromptSubmitted,
                json!({"text": "SECRET_PROMPT_SENTINEL"}),
            ))
            .await
            .expect("prompt should append");
        drop(store);

        generate_timeline(&database, &output, 50)
            .await
            .expect("timeline should render");
        let html = std::fs::read_to_string(&output).expect("timeline should read");
        assert!(html.contains("Qwen Code"));
        assert!(!html.contains("SECRET_WORKSPACE"));
        assert!(!html.contains("SECRET_PROMPT_SENTINEL"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output)
                    .expect("timeline metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            std::fs::set_permissions(&output, std::os::unix::fs::PermissionsExt::from_mode(0o644))
                .expect("make prior output public for replacement test");
            generate_timeline(&database, &output, 50)
                .await
                .expect("public prior output should be atomically replaced");
            assert_eq!(
                std::fs::metadata(&output)
                    .expect("replacement metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );

            let hardlink = directory.path().join("database-hardlink.html");
            std::fs::hard_link(&database, &hardlink).expect("database hardlink");
            let hardlink_error = generate_timeline(&database, &hardlink, 50)
                .await
                .expect_err("database hardlink must be rejected");
            assert!(hardlink_error.to_string().contains("must not replace"));
        }

        let same_file = generate_timeline(&database, &database, 50)
            .await
            .expect_err("timeline must not replace database");
        assert!(same_file.to_string().contains("must not replace"));
        assert!(SqliteEventStore::open_read_only(database).is_ok());
    }

    #[test]
    fn rejects_unknown_config_fields() {
        let error = toml::from_str::<UrdiaConfig>(
            r#"
                [[provider]]
                id = "custom-agent"
                name = "Custom Agent"
                command = "agent"
                shell = "agent --acp"
            "#,
        )
        .expect_err("shell command fields must not be accepted");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn config_errors_do_not_repeat_source_values() {
        const SENTINEL: &str = "REVIEW_SENTINEL_NOT_A_REAL_SECRET";
        let directory = tempfile::tempdir().expect("temporary directory should open");
        let path = directory.path().join("providers.toml");
        std::fs::write(
            &path,
            format!(
                r#"
                    [[provider]]
                    id = "custom-agent"
                    name = "Custom Agent"
                    command = "agent"
                    token = "{SENTINEL}"
                "#
            ),
        )
        .expect("test config should write");

        let error = build_registry(Some(&path)).expect_err("unknown field should fail");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("invalid TOML near byte"));
        assert!(!diagnostic.contains(SENTINEL));
        assert!(!diagnostic.contains("token ="));
    }

    #[tokio::test]
    async fn unknown_provider_creates_no_database() {
        let directory = tempfile::tempdir().expect("temporary directory should open");
        let database = directory.path().join("events.sqlite");
        let result = run_provider(
            &ProviderRegistry::new(),
            "missing",
            urdia_core::SessionControls::default(),
            directory.path().to_path_buf(),
            "test".to_owned(),
            database.clone(),
            false,
            false,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(!database.exists());
    }

    #[tokio::test]
    async fn unavailable_provider_creates_no_database() {
        let directory = tempfile::tempdir().expect("temporary directory should open");
        let database = directory.path().join("events.sqlite");
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(AcpProvider::with_metadata(
                "offline",
                "Offline",
                AcpCommand::new("/definitely/not/an/agent"),
            ))
            .expect("provider should register");
        let result = run_provider(
            &registry,
            "offline",
            urdia_core::SessionControls::default(),
            directory.path().to_path_buf(),
            "test".to_owned(),
            database.clone(),
            false,
            false,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(!database.exists());
    }
}
