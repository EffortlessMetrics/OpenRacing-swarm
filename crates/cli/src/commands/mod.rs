//! Command implementations for wheelctl CLI

pub mod device;
pub mod diag;
pub mod game;
pub mod hardware;
pub mod health;
pub mod moza;
pub mod plugin;
pub mod profile;
pub mod safety;
pub mod telemetry;

use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub enum DeviceCommands {
    /// List all connected devices
    List {
        /// Show detailed device information
        #[arg(short, long)]
        detailed: bool,
        /// List only observe-only HID endpoints, without service/mock devices
        #[arg(long)]
        hid_observe_only: bool,
        /// Write the device list receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Show device status and telemetry
    Status {
        /// Device ID or name
        device: String,
        /// Optional Moza lane artifact directory used to report descriptor trust
        #[arg(long)]
        moza_lane: Option<std::path::PathBuf>,
        /// Write the device status receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
        /// Watch status in real-time
        #[arg(short, long)]
        watch: bool,
    },

    /// Calibrate device (center, DOR, pedals)
    Calibrate {
        /// Device ID or name
        device: String,
        /// Calibration type
        #[arg(value_enum)]
        calibration_type: CalibrationType,
        /// Skip interactive prompts
        #[arg(short, long)]
        yes: bool,
    },

    /// Reset device to safe state
    Reset {
        /// Device ID or name
        device: String,
        /// Force reset without confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// List available profiles
    List {
        /// Filter by game
        #[arg(short, long)]
        game: Option<String>,
        /// Filter by car
        #[arg(short, long)]
        car: Option<String>,
    },

    /// Show profile details
    Show {
        /// Profile path or ID
        profile: String,
    },

    /// Apply profile to device
    Apply {
        /// Device ID or name
        device: String,
        /// Profile path or ID
        profile: String,
        /// Skip validation
        #[arg(long)]
        skip_validation: bool,
    },

    /// Create new profile
    Create {
        /// Profile file path
        path: String,
        /// Base profile to copy from
        #[arg(long)]
        from: Option<String>,
        /// Game scope
        #[arg(long)]
        game: Option<String>,
        /// Car scope
        #[arg(long)]
        car: Option<String>,
    },

    /// Edit profile interactively
    Edit {
        /// Profile path or ID
        profile: String,
        /// Field to edit (e.g., base.ffbGain)
        #[arg(long)]
        field: Option<String>,
        /// New value
        #[arg(long)]
        value: Option<String>,
    },

    /// Validate profile
    Validate {
        /// Profile path
        path: String,
        /// Show detailed validation info
        #[arg(short, long)]
        detailed: bool,
    },

    /// Export profile
    Export {
        /// Profile path or ID
        profile: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
        /// Include signature
        #[arg(long)]
        signed: bool,
    },

    /// Import profile
    Import {
        /// Profile file path
        path: String,
        /// Target directory
        #[arg(short, long)]
        target: Option<String>,
        /// Verify signature
        #[arg(long)]
        verify: bool,
    },
}

#[derive(Subcommand)]
pub enum DiagCommands {
    /// Run system diagnostics
    Test {
        /// Device ID or name
        #[arg(short, long)]
        device: Option<String>,
        /// Test type
        #[arg(value_enum)]
        test_type: Option<TestType>,
    },

    /// Record blackbox data
    Record {
        /// Device ID or name
        device: String,
        /// Recording duration in seconds
        #[arg(short, long, default_value = "120")]
        duration: u64,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Replay blackbox recording
    Replay {
        /// Blackbox file path
        file: String,
        /// Show frame-by-frame output
        #[arg(short, long)]
        detailed: bool,
    },

    /// Generate support bundle
    Support {
        /// Include blackbox recording
        #[arg(short, long)]
        blackbox: bool,
        /// Include Moza lane receipt verification summaries from this directory
        #[arg(long)]
        moza_lane: Option<String>,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Show performance metrics
    Metrics {
        /// Device ID or name
        device: Option<String>,
        /// Watch metrics in real-time
        #[arg(short, long)]
        watch: bool,
    },
}

#[derive(Subcommand)]
pub enum GameCommands {
    /// List supported games
    List {
        /// Show configuration details
        #[arg(short, long)]
        detailed: bool,
    },

    /// Configure game for telemetry
    Configure {
        /// Game ID
        game: String,
        /// Game installation path
        #[arg(short, long)]
        path: Option<String>,
        /// Enable auto-configuration
        #[arg(long)]
        auto: bool,
    },

    /// Show game status
    Status {
        /// Show telemetry data
        #[arg(short, long)]
        telemetry: bool,
    },

    /// Test telemetry connection
    Test {
        /// Game ID
        game: String,
        /// Test duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
    },
}

#[derive(Subcommand)]
pub enum TelemetryCommands {
    /// Probe telemetry transport for a game
    Probe {
        /// Game ID
        #[arg(long)]
        game: String,
        /// Handshake endpoint host:port
        #[arg(long, default_value = "127.0.0.1:9000")]
        endpoint: String,
        /// Timeout per probe attempt in milliseconds
        #[arg(long, default_value = "400")]
        timeout_ms: u64,
        /// Number of handshake attempts
        #[arg(long, default_value = "3")]
        attempts: u32,
    },

    /// Capture raw UDP telemetry packets to a binary file
    Capture {
        /// Game ID
        #[arg(long)]
        game: String,
        /// Local UDP listen port
        #[arg(long, default_value = "9000")]
        port: u16,
        /// Capture duration in seconds
        #[arg(long, default_value = "10")]
        duration: u64,
        /// Output file path
        #[arg(long)]
        out: String,
        /// Maximum payload bytes to store per packet
        #[arg(long, default_value = "2048")]
        max_payload: usize,
    },

    /// Record normalized telemetry snapshots to JSONL with safety provenance
    Record {
        /// Game or bridge ID that produced the normalized telemetry
        #[arg(long)]
        game: String,
        /// Telemetry source classification: real_game or simhub_bridge
        #[arg(long, default_value = "real_game")]
        telemetry_source: String,
        /// JSON/JSONL file containing normalized telemetry snapshots
        #[arg(long)]
        input: Option<String>,
        /// Listen for live SimHub JSON UDP and record normalized snapshots
        #[arg(long)]
        live_simhub: bool,
        /// Local UDP listen port for --live-simhub
        #[arg(long, default_value = "5555")]
        port: u16,
        /// Output JSONL recording path
        #[arg(long)]
        out: String,
        /// Stable session ID to stamp on every output record
        #[arg(long)]
        session_id: Option<String>,
        /// Recording duration in milliseconds
        #[arg(long, default_value = "0")]
        duration_ms: u64,
    },

    /// Replay normalized telemetry into a virtual FFB output log with no hardware writes
    VirtualFfbLog {
        /// JSON/JSONL normalized telemetry recording or fixture
        #[arg(long)]
        input: String,
        /// Output JSONL virtual FFB log path; ci/hardware/** is refused
        #[arg(long)]
        out: String,
        /// Stable virtual writer session ID
        #[arg(long)]
        session_id: Option<String>,
        /// Maximum absolute virtual output percent
        #[arg(long, default_value = "2")]
        max_percent: f32,
        /// Virtual watchdog timeout in milliseconds
        #[arg(long, default_value = "100")]
        watchdog_timeout_ms: u64,
    },
}

#[derive(Subcommand)]
pub enum HardwareCommands {
    /// Inspect local hardware/tooling readiness without opening devices or sending writes
    Doctor {
        /// Write the hardware doctor receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Print the staged hardware bring-up rail for a device family
    BringupRail {
        /// Device-family adapter contract to include in the rail receipt
        #[arg(long, default_value = "generic-wheelbase")]
        family: String,
        /// Write the staged rail receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Scaffold a hardware validation lane from a device-family rail adapter
    #[command(subcommand)]
    Lane(Box<HardwareLaneCommands>),
}

#[derive(Subcommand)]
pub enum HardwareLaneCommands {
    /// Create a lane manifest, checklist, capture plan, and stage-gate scaffold
    Init {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-13
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Device-family adapter contract to use
        #[arg(long, default_value = "generic-wheelbase")]
        family: String,
        /// Declared primary topology/path for this lane
        #[arg(long, default_value = "unknown")]
        topology: String,
        /// Operator name recorded in the scaffold manifest
        #[arg(long, default_value = "Steven")]
        operator: String,
        /// Mark a logical role as required, adding it to the lane if needed
        #[arg(long = "required-role")]
        required_roles: Vec<String>,
        /// Mark a logical role as optional, adding it to the lane if needed
        #[arg(long = "optional-role")]
        optional_roles: Vec<String>,
        /// Override a role evidence artifact as role=relative/path
        #[arg(long = "role-artifact")]
        role_artifacts: Vec<String>,
        /// Override a role endpoint selector as role=selector
        #[arg(long = "role-endpoint")]
        role_endpoints: Vec<String>,
        /// Override a role connection path as role=path
        #[arg(long = "role-connection")]
        role_connections: Vec<String>,
        /// Replace scaffold files that already exist
        #[arg(long)]
        overwrite: bool,
        /// Write the lane-init receipt to this JSON file instead of <lane>/lane-init.json
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Inventory a scaffolded hardware lane without validating hardware claims
    Status {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-13
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Write the lane-status receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Update one declared role endpoint in the lane manifest after discovery
    SetRoleEndpoint {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-13
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Declared logical role to update
        #[arg(long)]
        role: String,
        /// Observed endpoint selector for that role
        #[arg(long)]
        endpoint: String,
        /// Write the role-endpoint update receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum MozaCommands {
    /// Create a Moza R5 validation lane manifest and capture directory
    InitLane {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// R5 wheelbase PID for this lane (hex, 0x0004 or 0x0014)
        #[arg(long, default_value = "0x0014")]
        wheelbase_pid: String,
        /// Operator name recorded in manifest.json
        #[arg(long, default_value = "Steven")]
        operator: String,
        /// Replace an existing manifest.json
        #[arg(long)]
        overwrite: bool,
    },

    /// Probe connected Moza HID devices without sending FFB writes
    Probe {
        /// Write the probe receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Summarize Moza device and lane readiness without opening HID devices
    Status {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Optional lane artifact directory to include verifier summaries
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Write the status receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Capture descriptor metadata for connected Moza HID devices
    Descriptor {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Include full descriptor hex when available
        #[arg(long)]
        descriptor_hex: bool,
        /// Operator-supplied HID report descriptor hex, used when the OS cannot expose raw bytes
        #[arg(long)]
        report_descriptor_hex: Option<String>,
        /// File containing operator-supplied HID report descriptor hex
        #[arg(long)]
        report_descriptor_hex_file: Option<std::path::PathBuf>,
        /// Raw binary HID report descriptor file, for example Linux sysfs report_descriptor
        #[arg(long)]
        report_descriptor_bin_file: Option<std::path::PathBuf>,
        /// Write the descriptor receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Capture input reports from one Moza HID device without FFB writes
    CaptureInput {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Capture duration in milliseconds
        #[arg(long, default_value = "1000")]
        duration_ms: u64,
        /// HID read timeout in milliseconds
        #[arg(long, default_value = "100")]
        read_timeout_ms: i32,
        /// Write captured reports as JSON Lines to this file
        #[arg(long)]
        json_out: std::path::PathBuf,
    },

    /// Validate captured Moza input JSONL through the parser without hardware writes
    ValidateCapture {
        /// JSON Lines file produced by `wheelctl moza capture-input`
        #[arg(long)]
        capture: std::path::PathBuf,
        /// Optional PID override (hex, e.g. 0x0014). Defaults to per-line product_id
        #[arg(long)]
        pid: Option<String>,
        /// Write the validation receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Analyze raw byte/word movement in a captured Moza input JSONL without hardware writes
    AnalyzeCapture {
        /// JSON Lines file produced by `wheelctl moza capture-input`
        #[arg(long)]
        capture: std::path::PathBuf,
        /// Write the analysis receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Compare required passive lane captures against idle without hardware writes
    AnalyzeLane {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Write the lane analysis receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Sync manifest logical-control semantic_status fields from stored captures
    SyncRoleStatus {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Verify manifest statuses are current without writing manifest.json
        #[arg(long)]
        check: bool,
        /// Write the sync receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Validate every required Moza lane capture through the parser without hardware writes
    ValidateCaptures {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Write the aggregate validation receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Summarize whether a Moza lane is ready for zero-torque or FFB output
    PreOutputReadiness {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Write the readiness receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Promote a validated Moza capture JSONL into a parser fixture file
    PromoteFixture {
        /// JSON Lines file produced by `wheelctl moza capture-input`
        #[arg(long)]
        capture: std::path::PathBuf,
        /// Fixture identifier recorded in the output JSON
        #[arg(long)]
        fixture_id: String,
        /// Parser fixture JSON file to write
        #[arg(long)]
        fixture_out: std::path::PathBuf,
        /// Optional PID override (hex, e.g. 0x0014). Defaults to per-line product_id
        #[arg(long)]
        pid: Option<String>,
        /// Maximum reports to include in the fixture
        #[arg(long, default_value = "256")]
        max_reports: usize,
        /// Allow replacing an existing fixture file
        #[arg(long)]
        overwrite: bool,
        /// Write the promotion receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Promote every required Moza lane capture into parser fixture files
    PromoteFixtures {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Directory that will receive sanitized parser fixture JSON files
        #[arg(long)]
        fixture_dir: std::path::PathBuf,
        /// Maximum reports to include per fixture
        #[arg(long, default_value = "256")]
        max_reports: usize,
        /// Allow replacing existing fixture files
        #[arg(long)]
        overwrite: bool,
        /// Write the aggregate promotion receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Send bounded zero-torque output reports to a Moza wheelbase
    Zero {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Hardware lane directory with a passing pre-output-readiness receipt
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Moza wheelbase PID for --dry-run (hex, e.g. 0x0014)
        #[arg(long)]
        pid: Option<String>,
        /// Zero-output report strategy to use
        #[arg(long, value_enum, default_value_t = MozaZeroOutputStrategy::DirectReport0x20)]
        strategy: MozaZeroOutputStrategy,
        /// Build the zero-torque receipt without opening or writing a HID device
        #[arg(long)]
        dry_run: bool,
        /// Explicit acknowledgement required before actual zero-torque writes
        #[arg(long)]
        confirm_zero_torque: bool,
        /// Number of zero reports to send before the final-zero attempt
        #[arg(long, default_value = "100")]
        repeat: u32,
        /// Output rate in Hz, bounded to 1..=1000
        #[arg(long, default_value = "1000")]
        hz: u32,
        /// Watchdog timeout in milliseconds before forcing final zero
        #[arg(long, default_value = "100")]
        watchdog_timeout_ms: u64,
        /// Write the zero-torque proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Inject a watchdog timeout and prove the response is final zero
    WatchdogProof {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Hardware lane directory with passing pre-output and zero-torque receipts
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Moza wheelbase PID for --dry-run (hex, e.g. 0x0014)
        #[arg(long)]
        pid: Option<String>,
        /// Zero-output report strategy to use
        #[arg(long, value_enum, default_value_t = MozaZeroOutputStrategy::DirectReport0x20)]
        strategy: MozaZeroOutputStrategy,
        /// Build the watchdog proof receipt without opening or writing a HID device
        #[arg(long)]
        dry_run: bool,
        /// Explicit acknowledgement required before the watchdog timeout test
        #[arg(long)]
        confirm_watchdog_test: bool,
        /// Number of zero reports to send before the injected watchdog timeout
        #[arg(long, default_value = "3")]
        pre_zero_count: u32,
        /// Output rate in Hz, bounded to 1..=1000
        #[arg(long, default_value = "1000")]
        hz: u32,
        /// Watchdog timeout in milliseconds to inject
        #[arg(long, default_value = "100")]
        watchdog_timeout_ms: u64,
        /// Write the watchdog proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Write zero torque until a disconnect is observed, then attempt final zero
    DisconnectProof {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Hardware lane directory with passing pre-output and zero-torque receipts
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Moza wheelbase PID for --dry-run (hex, e.g. 0x0014)
        #[arg(long)]
        pid: Option<String>,
        /// Zero-output report strategy to use
        #[arg(long, value_enum, default_value_t = MozaZeroOutputStrategy::DirectReport0x20)]
        strategy: MozaZeroOutputStrategy,
        /// Build the disconnect proof receipt without opening or writing a HID device
        #[arg(long)]
        dry_run: bool,
        /// Explicit acknowledgement required before the operator disconnect test
        #[arg(long)]
        confirm_disconnect_test: bool,
        /// Maximum time to wait for disconnect before failing the receipt
        #[arg(long, default_value = "10000")]
        max_duration_ms: u64,
        /// Output rate in Hz, bounded to 1..=1000
        #[arg(long, default_value = "1000")]
        hz: u32,
        /// Write the disconnect proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Run the staged Moza wheelbase handshake in off or standard mode
    Init {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Hardware lane directory with passing zero-stage verification and audit receipts
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Moza wheelbase PID for --dry-run (hex, e.g. 0x0014)
        #[arg(long)]
        pid: Option<String>,
        /// Handshake FFB mode. Direct mode is intentionally not available here.
        #[arg(long, value_enum, default_value_t = MozaInitMode::Off)]
        mode: MozaInitMode,
        /// Build the init receipt without opening or writing a HID device
        #[arg(long)]
        dry_run: bool,
        /// Explicit acknowledgement required before actual init feature-report writes
        #[arg(long)]
        confirm_init: bool,
        /// Write the init receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Send a gated low-torque ladder after a passing real zero-torque proof
    TorqueTest {
        /// Device selector: HID path, PID, or VID:PID
        #[arg(long)]
        device: Option<String>,
        /// Moza wheelbase PID for --dry-run (hex, e.g. 0x0014)
        #[arg(long)]
        pid: Option<String>,
        /// Passing real zero-torque proof receipt from `wheelctl moza zero`
        #[arg(long)]
        zero_proof: Option<std::path::PathBuf>,
        /// Descriptor receipt proving a trusted R5 descriptor for direct report writes
        #[arg(long)]
        descriptor: Option<std::path::PathBuf>,
        /// Lane artifact directory containing init-off.json and init-standard.json
        #[arg(long)]
        lane: Option<std::path::PathBuf>,
        /// Passing off-mode init receipt from `wheelctl moza init --mode off`
        #[arg(long)]
        init_off: Option<std::path::PathBuf>,
        /// Passing standard-mode init receipt from `wheelctl moza init --mode standard`
        #[arg(long)]
        init_standard: Option<std::path::PathBuf>,
        /// Low-torque output strategy to prove
        #[arg(long, value_enum, default_value_t = MozaLowTorqueStrategy::DirectReport0x20)]
        strategy: MozaLowTorqueStrategy,
        /// Build the low-torque receipt without opening or writing a HID device
        #[arg(long)]
        dry_run: bool,
        /// Explicit acknowledgement required before actual low-torque writes
        #[arg(long)]
        confirm_low_torque: bool,
        /// Explicitly allow direct report writes when descriptor trust is unavailable
        #[arg(long)]
        explicit_operator_override: bool,
        /// Maximum torque percent for the ladder, bounded to 0.1..=2.0
        #[arg(long, default_value = "2")]
        max_percent: f32,
        /// Duration of each ladder stage in milliseconds
        #[arg(long, default_value = "250")]
        duration_ms: u64,
        /// Output rate in Hz, bounded to 1..=1000
        #[arg(long, default_value = "1000")]
        hz: u32,
        /// Write the low-torque proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Write a non-claiming starter receipt for manual Pit House or simulator evidence
    ReceiptTemplate {
        /// Receipt template kind to write
        #[arg(long, value_enum)]
        kind: MozaReceiptTemplateKind,
        /// JSON file to write
        #[arg(long)]
        json_out: std::path::PathBuf,
        /// Replace an existing template file
        #[arg(long)]
        overwrite: bool,
    },

    /// Write an observed Pit House UI/process state receipt
    PitHouseObservation {
        /// Pit House coexistence case being observed
        #[arg(long, value_enum)]
        case: MozaPitHouseObservationCase,
        /// Evidence source used for this operator observation
        #[arg(long, value_enum, default_value_t = MozaPitHouseEvidenceKind::ProcessWindowSnapshot)]
        evidence_kind: MozaPitHouseEvidenceKind,
        /// Lane-relative screenshot, video, or process/window snapshot artifact for this observation
        #[arg(long)]
        evidence_artifact: Option<std::path::PathBuf>,
        /// Operator or host label recorded in the receipt
        #[arg(long, default_value = "Steven")]
        operator: String,
        /// Short operator evidence note; use artifact paths for screenshots/videos if needed
        #[arg(long)]
        evidence: String,
        /// Write the Pit House observation receipt to this JSON file
        #[arg(long)]
        json_out: std::path::PathBuf,
        /// Replace an existing observation receipt
        #[arg(long)]
        overwrite: bool,
    },

    /// Build one Pit House coexistence case artifact from source receipts
    PitHouseCase {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Pit House coexistence case to build
        #[arg(long, value_enum)]
        case: MozaPitHouseObservationCase,
        /// Lane-relative observation receipt from `pit-house-observation`
        #[arg(long)]
        observation_artifact: std::path::PathBuf,
        /// Short operator evidence note for this case artifact
        #[arg(long)]
        evidence: String,
        /// Write the case artifact to this JSON file
        #[arg(long)]
        json_out: std::path::PathBuf,
        /// Replace an existing case artifact
        #[arg(long)]
        overwrite: bool,
    },

    /// Build a Pit House coexistence receipt from observed case artifact files
    PitHouseProof {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Artifact for Pit House closed case
        #[arg(long)]
        closed_artifact: std::path::PathBuf,
        /// Artifact for Pit House open, idle, standard-mode case
        #[arg(long)]
        open_standard_artifact: std::path::PathBuf,
        /// Artifact for Pit House open, direct-mode case
        #[arg(long)]
        direct_artifact: std::path::PathBuf,
        /// Artifact for Pit House mode-change-during-run case
        #[arg(long)]
        mode_change_artifact: std::path::PathBuf,
        /// Artifact for Pit House firmware/update page case
        #[arg(long)]
        firmware_page_artifact: std::path::PathBuf,
        /// Shared-control risk outcome: detected, warned, or documented_limit
        #[arg(long, default_value = "warned")]
        shared_control_risk: String,
        /// Write the Pit House proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
        /// Replace an existing proof receipt
        #[arg(long)]
        overwrite: bool,
    },

    /// Build a telemetry-only simulator proof receipt from normalized snapshots
    SimulatorTelemetryProof {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Simulator or bridge name
        #[arg(long)]
        game: String,
        /// Telemetry source: real_game or simhub_bridge
        #[arg(long, default_value = "simhub_bridge")]
        telemetry_source: String,
        /// Lane-relative normalized telemetry recording artifact
        #[arg(long)]
        recorder_artifact: std::path::PathBuf,
        /// Recording duration in milliseconds
        #[arg(long)]
        duration_ms: u64,
        /// Write the simulator telemetry proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
        /// Replace an existing proof receipt
        #[arg(long)]
        overwrite: bool,
    },

    /// Build a bounded simulator-to-Moza FFB smoke proof from output logs
    SimulatorFfbSmoke {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Simulator or bridge name
        #[arg(long)]
        game: String,
        /// Telemetry source: real_game or simhub_bridge
        #[arg(long, default_value = "simhub_bridge")]
        telemetry_source: String,
        /// Lane-relative output log artifact
        #[arg(long)]
        output_log_artifact: std::path::PathBuf,
        /// Bounded simulator FFB output strategy
        #[arg(long, value_enum, default_value_t = MozaLowTorqueStrategy::DirectReport0x20)]
        strategy: MozaLowTorqueStrategy,
        /// Descriptor trust was established before simulator FFB
        #[arg(long)]
        descriptor_trusted: bool,
        /// Explicit operator override allowed direct-mode FFB without descriptor trust
        #[arg(long)]
        explicit_operator_override: bool,
        /// Watchdog timeout used during the smoke run
        #[arg(long)]
        watchdog_timeout_ms: u64,
        /// Stop event cleared output
        #[arg(long)]
        stop_cleared_output: bool,
        /// Pause event cleared output
        #[arg(long)]
        pause_cleared_output: bool,
        /// Game exit cleared output
        #[arg(long)]
        game_exit_cleared_output: bool,
        /// Write the simulator FFB smoke proof receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
        /// Replace an existing proof receipt
        #[arg(long)]
        overwrite: bool,
    },

    /// Promote manifest completion state only after a live bundle verification passes
    PromoteManifest {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Evidence stage to promote the manifest to
        #[arg(long, value_enum)]
        stage: MozaBundleStage,
        /// Write the promotion receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Verify a Moza hardware lane receipt bundle before claiming readiness
    VerifyBundle {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Evidence stage to require
        #[arg(long, value_enum, default_value_t = MozaBundleStage::Passive)]
        stage: MozaBundleStage,
        /// Write the verification receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },

    /// Audit stored verification and manifest-promotion receipts after promotion
    AuditLane {
        /// Lane artifact directory, e.g. ci/hardware/moza-r5/2026-05-06
        #[arg(long)]
        lane: std::path::PathBuf,
        /// Promotion stage whose stored receipts must be present and consistent
        #[arg(long, value_enum, default_value_t = MozaBundleStage::Passive)]
        stage: MozaBundleStage,
        /// Write the audit receipt to this JSON file
        #[arg(long)]
        json_out: Option<std::path::PathBuf>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaInitMode {
    /// Start reports but leave force feedback disabled
    Off,
    /// Start reports and select vendor standard/PIDFF mode
    Standard,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaBundleStage {
    /// Enumeration, descriptor, input captures, and parser validation only
    Passive,
    /// Passive evidence plus real zero-torque, watchdog, and disconnect proof
    Zero,
    /// OpenRacing-owned native control receipts without external compatibility gates
    #[value(
        name = "openracing-control-ready",
        alias = "native-control-ready",
        alias = "openracing_control_ready",
        alias = "native_control_ready"
    )]
    OpenRacingControlReady,
    /// Native control plus external simulator and vendor coexistence evidence
    SmokeReady,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaZeroOutputStrategy {
    /// Moza proprietary direct torque report 0x20 encoded as zero torque
    DirectReport0x20,
    /// Standard PIDFF Device Control report 0x0C with Stop All Effects
    PidffStopAll,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaLowTorqueStrategy {
    /// Moza proprietary direct torque report 0x20
    #[value(name = "direct-report-0x20", alias = "direct-report0x20")]
    DirectReport0x20,
    /// Standard HID PIDFF bounded effect path
    PidffBoundedEffect,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaReceiptTemplateKind {
    /// Pit House coexistence matrix receipt
    PitHouse,
    /// Telemetry-only real simulator receipt
    SimulatorTelemetry,
    /// Bounded simulator-to-Moza FFB smoke receipt
    SimulatorFfb,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaPitHouseObservationCase {
    /// Pit House closed before OpenRacing staged handshake
    Closed,
    /// Pit House open and idle while OpenRacing uses standard mode
    OpenStandard,
    /// Pit House open while OpenRacing direct mode is requested
    OpenDirect,
    /// Pit House changes mode during a bounded run
    ModeChange,
    /// Pit House firmware/update page is open
    FirmwarePage,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MozaPitHouseEvidenceKind {
    /// Operator-written notes
    OperatorNotes,
    /// Screenshot or saved image from the operator
    OperatorScreenshot,
    /// Video or screen recording from the operator
    OperatorVideo,
    /// Process/window snapshot captured by tooling
    ProcessWindowSnapshot,
}

#[derive(Subcommand)]
pub enum SafetyCommands {
    /// Enable high torque mode
    Enable {
        /// Device ID or name
        device: String,
        /// Skip safety confirmation
        #[arg(long)]
        force: bool,
    },

    /// Emergency stop all devices
    Stop {
        /// Specific device ID or name
        device: Option<String>,
    },

    /// Show safety status
    Status {
        /// Device ID or name
        device: Option<String>,
    },

    /// Set torque limits
    Limit {
        /// Device ID or name
        device: String,
        /// Maximum torque in Nm
        torque: f32,
        /// Apply to all profiles
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand)]
pub enum PluginCommands {
    /// List available plugins from registry
    List {
        /// Filter by category (e.g., ffb, telemetry, led)
        #[arg(short, long)]
        category: Option<String>,
    },

    /// Search for plugins by name or description
    Search {
        /// Search query
        query: String,
    },

    /// Install a plugin from the registry
    Install {
        /// Plugin ID or name
        plugin_id: String,
        /// Specific version to install (defaults to latest)
        #[arg(long)]
        version: Option<String>,
    },

    /// Uninstall a plugin
    Uninstall {
        /// Plugin ID or name
        plugin_id: String,
        /// Force uninstall without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Show detailed plugin information
    Info {
        /// Plugin ID or name
        plugin_id: String,
        /// Show info for specific version
        #[arg(long)]
        version: Option<String>,
    },

    /// Verify an installed plugin's integrity and signature
    Verify {
        /// Plugin ID or name
        plugin_id: String,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum CalibrationType {
    Center,
    Dor,
    Pedals,
    All,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum TestType {
    Motor,
    Encoder,
    Usb,
    Thermal,
    All,
}
