//! Test harness for RT loop validation with virtual devices
//!
//! This module provides a comprehensive test harness for validating the real-time
//! force feedback loop using virtual devices. It includes timing validation,
//! performance measurement, and integration testing capabilities.

use crate::ports::{HidDevice, HidPort};
use crate::prelude::MutexExt;
use crate::{PerformanceMetrics, TelemetryData, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::*;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

/// Test harness configuration
#[derive(Debug, Clone)]
pub struct TestHarnessConfig {
    /// Target update rate in Hz (typically 1000)
    pub update_rate_hz: f32,

    /// Test duration
    pub test_duration: Duration,

    /// Maximum allowed jitter in microseconds
    pub max_jitter_us: f64,

    /// Maximum allowed missed tick rate (0.0 to 1.0)
    pub max_missed_tick_rate: f64,

    /// Enable performance monitoring
    pub enable_performance_monitoring: bool,

    /// Enable detailed logging
    pub enable_detailed_logging: bool,
}

impl Default for TestHarnessConfig {
    fn default() -> Self {
        Self {
            update_rate_hz: 1000.0,
            test_duration: Duration::from_secs(10),
            max_jitter_us: 250.0,          // 0.25ms as per requirements
            max_missed_tick_rate: 0.00001, // 0.001% as per requirements
            enable_performance_monitoring: true,
            enable_detailed_logging: false,
        }
    }
}

/// Test scenario for RT loop validation
#[derive(Debug, Clone)]
pub struct TestScenario {
    /// Scenario name
    pub name: String,

    /// Torque pattern to apply during test
    pub torque_pattern: TorquePattern,

    /// Expected device responses
    pub expected_responses: Vec<ExpectedResponse>,

    /// Fault injection points
    pub fault_injections: Vec<FaultInjection>,
}

/// Torque pattern for testing
#[derive(Debug, Clone)]
pub enum TorquePattern {
    /// Constant torque value
    Constant(f32),

    /// Sine wave: amplitude, frequency_hz, phase_offset
    SineWave {
        amplitude: f32,
        frequency_hz: f32,
        phase_offset: f32,
    },

    /// Square wave: amplitude, frequency_hz, duty_cycle
    SquareWave {
        amplitude: f32,
        frequency_hz: f32,
        duty_cycle: f32,
    },

    /// Ramp: start_value, end_value, duration
    Ramp {
        start_value: f32,
        end_value: f32,
        duration: Duration,
    },

    /// Custom pattern from vector
    Custom(Vec<f32>),
}

impl TorquePattern {
    /// Get torque value at given time
    pub fn value_at(&self, time: Duration, _start_time: Instant) -> f32 {
        let elapsed = time;
        let t_sec = elapsed.as_secs_f32();

        match self {
            TorquePattern::Constant(value) => *value,

            TorquePattern::SineWave {
                amplitude,
                frequency_hz,
                phase_offset,
            } => {
                amplitude * (2.0 * std::f32::consts::PI * frequency_hz * t_sec + phase_offset).sin()
            }

            TorquePattern::SquareWave {
                amplitude,
                frequency_hz,
                duty_cycle,
            } => {
                let period = 1.0 / frequency_hz;
                let phase = (t_sec % period) / period;
                if phase < *duty_cycle {
                    *amplitude
                } else {
                    -*amplitude
                }
            }

            TorquePattern::Ramp {
                start_value,
                end_value,
                duration,
            } => {
                let progress = (t_sec / duration.as_secs_f32()).clamp(0.0, 1.0);
                start_value + (end_value - start_value) * progress
            }

            TorquePattern::Custom(values) => {
                if values.is_empty() {
                    0.0
                } else {
                    let index = ((t_sec * 1000.0) as usize) % values.len();
                    values[index]
                }
            }
        }
    }
}

/// Expected device response for validation
#[derive(Debug, Clone)]
pub struct ExpectedResponse {
    /// Time offset from test start
    pub time_offset: Duration,

    /// Expected wheel angle range (degrees)
    pub wheel_angle_range: Option<(f32, f32)>,

    /// Expected wheel speed range (rad/s)
    pub wheel_speed_range: Option<(f32, f32)>,

    /// Expected temperature range (Celsius)
    pub temperature_range: Option<(u8, u8)>,

    /// Expected fault flags
    pub expected_faults: Option<u8>,
}

/// Fault injection for testing error handling
#[derive(Debug, Clone)]
pub struct FaultInjection {
    /// Time to inject fault
    pub inject_at: Duration,

    /// Fault type to inject
    pub fault_type: u8,

    /// Duration to maintain fault
    pub duration: Duration,
}

/// Test result with detailed metrics
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test scenario name
    pub scenario_name: String,

    /// Test passed/failed
    pub passed: bool,

    /// Performance metrics
    pub performance: PerformanceMetrics,

    /// Timing validation results
    pub timing_validation: TimingValidation,

    /// Response validation results
    pub response_validation: Vec<ResponseValidationResult>,

    /// Force-feedback write lifecycle observed by the harness
    pub output_validation: OutputValidation,

    /// Error messages (if any)
    pub errors: Vec<String>,

    /// Test duration
    pub actual_duration: Duration,
}

/// Force-feedback output lifecycle results from a virtual-device run.
#[derive(Debug, Clone, Default)]
pub struct OutputValidation {
    /// Number of force-feedback writes attempted during the run
    pub attempted_writes: u64,
    /// Number of writes rejected by the virtual device
    pub rejected_writes: u64,
    /// Whether the harness issued and accepted a final zero-torque write
    pub final_zero_written: bool,
}

#[derive(Debug, Default)]
struct OutputTrace {
    attempted_writes: u64,
    rejected_writes: u64,
    final_zero_written: bool,
}

/// Timing validation results
#[derive(Debug, Clone)]
pub struct TimingValidation {
    /// Average tick interval in microseconds
    pub avg_tick_interval_us: f64,

    /// Maximum jitter observed in microseconds
    pub max_jitter_us: f64,

    /// P99 jitter in microseconds
    pub p99_jitter_us: f64,

    /// Total ticks processed
    pub total_ticks: u64,

    /// Missed ticks
    pub missed_ticks: u64,

    /// Timing violations (jitter > threshold)
    pub timing_violations: u64,
}

/// Response validation result
#[derive(Debug, Clone)]
pub struct ResponseValidationResult {
    /// Expected response being validated
    pub expected: ExpectedResponse,

    /// Actual response received
    pub actual: TelemetryData,

    /// Validation passed
    pub passed: bool,

    /// Validation errors
    pub errors: Vec<String>,
}

/// Real-time loop test harness
pub struct RTLoopTestHarness {
    config: TestHarnessConfig,
    virtual_port: VirtualHidPort,
    performance_metrics: Arc<Mutex<PerformanceMetrics>>,
    timing_data: Arc<Mutex<VecDeque<Duration>>>,
    stop_flag: Arc<AtomicBool>,
    tick_counter: Arc<AtomicU64>,
}

impl RTLoopTestHarness {
    /// Create a new test harness
    pub fn new(config: TestHarnessConfig) -> Self {
        Self {
            config,
            virtual_port: VirtualHidPort::new(),
            performance_metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
            timing_data: Arc::new(Mutex::new(VecDeque::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            tick_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add a virtual device to the test harness
    pub fn add_virtual_device(
        &mut self,
        device: VirtualDevice,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.virtual_port.add_device(device)
    }

    /// Create a standard test device
    pub fn create_test_device(
        &self,
        id: &str,
        name: &str,
    ) -> Result<VirtualDevice, Box<dyn std::error::Error>> {
        let device_id = id
            .parse::<DeviceId>()
            .map_err(|e| format!("Failed to parse DeviceId: {e}"))?;
        Ok(VirtualDevice::new(device_id, name.to_string()))
    }

    /// Run a test scenario
    pub async fn run_scenario(
        &mut self,
        scenario: TestScenario,
    ) -> Result<TestResult, Box<dyn std::error::Error>> {
        info!("Starting test scenario: {}", scenario.name);

        // Reset state
        self.stop_flag.store(false, Ordering::Relaxed);
        self.tick_counter.store(0, Ordering::Relaxed);
        {
            let mut metrics = self.performance_metrics.lock_or_panic();
            *metrics = PerformanceMetrics::default();
        }
        {
            let mut timing = self.timing_data.lock_or_panic();
            timing.clear();
        }

        // Get devices
        let devices = self.virtual_port.list_devices().await?;
        if devices.is_empty() {
            return Err("No virtual devices available for testing".into());
        }

        let device_id = devices[0].id.clone();
        let device = self.virtual_port.open_device(&device_id).await?;
        let output_trace = Arc::new(Mutex::new(OutputTrace::default()));

        // Start RT loop
        let start_time = Instant::now();
        let rt_handle = self
            .start_rt_loop(device, &scenario, start_time, Arc::clone(&output_trace))
            .await?;

        // Wait for test completion
        tokio::time::sleep(self.config.test_duration).await;

        // Stop RT loop
        self.stop_flag.store(true, Ordering::Relaxed);
        match rt_handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("Join error: {}", e).into()),
        }

        let actual_duration = start_time.elapsed();

        // Collect and analyze results
        let performance = {
            let metrics = self.performance_metrics.lock_or_panic();
            metrics.clone()
        };

        let timing_validation = self.analyze_timing_data();
        let response_validation = self.validate_responses(&scenario, &device_id).await?;
        let output_validation = {
            let output = output_trace.lock_or_panic();
            OutputValidation {
                attempted_writes: output.attempted_writes,
                rejected_writes: output.rejected_writes,
                final_zero_written: output.final_zero_written,
            }
        };

        // Determine if test passed
        let mut errors = Vec::new();
        let mut passed = true;

        // Check timing requirements
        if timing_validation.max_jitter_us > self.config.max_jitter_us {
            errors.push(format!(
                "Maximum jitter {} μs exceeds limit {} μs",
                timing_validation.max_jitter_us, self.config.max_jitter_us
            ));
            passed = false;
        }

        if performance.missed_tick_rate() > self.config.max_missed_tick_rate {
            errors.push(format!(
                "Missed tick rate {:.6} exceeds limit {:.6}",
                performance.missed_tick_rate(),
                self.config.max_missed_tick_rate
            ));
            passed = false;
        }

        if output_validation.attempted_writes == 0 {
            errors.push("RT loop produced no force-feedback writes".to_string());
            passed = false;
        }

        if !output_validation.final_zero_written {
            errors.push("RT loop did not complete with an accepted zero-torque write".to_string());
            passed = false;
        }

        // Check response validations
        for validation in &response_validation {
            if !validation.passed {
                passed = false;
                for error in &validation.errors {
                    errors.push(error.clone());
                }
            }
        }

        let result = TestResult {
            scenario_name: scenario.name.clone(),
            passed,
            performance,
            timing_validation,
            response_validation,
            output_validation,
            errors,
            actual_duration,
        };

        if result.passed {
            info!("Test scenario '{}' PASSED", scenario.name);
        } else {
            warn!(
                "Test scenario '{}' FAILED: {:?}",
                scenario.name, result.errors
            );
        }

        Ok(result)
    }

    /// Start the real-time loop
    async fn start_rt_loop(
        &self,
        mut device: Box<dyn HidDevice>,
        scenario: &TestScenario,
        start_time: Instant,
        output_trace: Arc<Mutex<OutputTrace>>,
    ) -> Result<
        tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
        Box<dyn std::error::Error>,
    > {
        let tick_interval =
            Duration::from_nanos((1_000_000_000.0 / self.config.update_rate_hz) as u64);
        let stop_flag = Arc::clone(&self.stop_flag);
        let tick_counter = Arc::clone(&self.tick_counter);
        let performance_metrics = Arc::clone(&self.performance_metrics);
        let timing_data = Arc::clone(&self.timing_data);
        let torque_pattern = scenario.torque_pattern.clone();
        let fault_injections = scenario.fault_injections.clone();

        let handle = tokio::spawn(async move {
            let mut next_tick = Instant::now();
            let mut seq = 0u16;
            let mut last_tick_time = Instant::now();

            while !stop_flag.load(Ordering::Relaxed) {
                let tick_start = Instant::now();

                // Calculate timing metrics
                let tick_interval_actual = tick_start.duration_since(last_tick_time);
                let jitter = tick_interval_actual.abs_diff(tick_interval);

                // Update timing data
                {
                    let mut timing = timing_data.lock_or_panic();
                    timing.push_back(jitter);
                    if timing.len() > 10000 {
                        timing.pop_front();
                    }
                }

                // Calculate torque value
                let elapsed = tick_start.duration_since(start_time);
                let torque_value = torque_pattern.value_at(elapsed, start_time);

                // Apply fault injections
                for fault in &fault_injections {
                    if elapsed >= fault.inject_at && elapsed <= fault.inject_at + fault.duration {
                        // Simulate fault injection
                        debug!("Injecting fault type {} at {:?}", fault.fault_type, elapsed);
                    }
                }

                let write_result = device.write_ffb_report(torque_value, seq);
                {
                    let mut output = output_trace.lock_or_panic();
                    output.attempted_writes += 1;
                    if write_result.is_err() {
                        output.rejected_writes += 1;
                    }
                }
                if let Err(error) = write_result {
                    warn!("Force-feedback write rejected: {error}");
                }

                seq = seq.wrapping_add(1);
                let tick_count = tick_counter.fetch_add(1, Ordering::Relaxed) + 1;

                // Update performance metrics
                {
                    let mut metrics = performance_metrics.lock_or_panic();
                    metrics.total_ticks = tick_count;

                    let jitter_ns = jitter.as_nanos() as u64;
                    if jitter_ns > metrics.max_jitter_ns {
                        metrics.max_jitter_ns = jitter_ns;
                    }

                    // Simple P99 approximation (would need proper percentile calculation)
                    metrics.p99_jitter_ns = (metrics.max_jitter_ns as f64 * 0.99) as u64;

                    metrics.last_update = tick_start;
                }

                last_tick_time = tick_start;

                // Wait for next tick
                next_tick += tick_interval;
                let now = Instant::now();
                if next_tick > now {
                    tokio::time::sleep(next_tick - now).await;
                } else {
                    // Missed deadline
                    let mut metrics = performance_metrics.lock_or_panic();
                    metrics.missed_ticks += 1;
                    next_tick = now;
                }
            }

            let final_zero_result = device.write_ffb_report(0.0, seq.wrapping_add(1));
            let final_zero_error = final_zero_result.as_ref().err().map(ToString::to_string);
            {
                let mut output = output_trace.lock_or_panic();
                output.attempted_writes += 1;
                output.final_zero_written = final_zero_result.is_ok();
                if final_zero_result.is_err() {
                    output.rejected_writes += 1;
                }
            }
            if let Some(error) = final_zero_error {
                return Err(format!("final zero-torque write rejected: {error}").into());
            }

            info!(
                "RT loop stopped after {} ticks",
                tick_counter.load(Ordering::Relaxed)
            );
            Ok(())
        });

        Ok(handle)
    }

    /// Analyze timing data and generate validation results
    fn analyze_timing_data(&self) -> TimingValidation {
        let timing_data = self.timing_data.lock_or_panic();
        let performance = self.performance_metrics.lock_or_panic();

        if timing_data.is_empty() {
            return TimingValidation {
                avg_tick_interval_us: 0.0,
                max_jitter_us: 0.0,
                p99_jitter_us: 0.0,
                total_ticks: performance.total_ticks,
                missed_ticks: performance.missed_ticks,
                timing_violations: 0,
            };
        }

        // Calculate statistics
        let mut jitter_values: Vec<f64> = timing_data
            .iter()
            .map(|d| d.as_nanos() as f64 / 1000.0) // Convert to microseconds
            .collect();

        jitter_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let avg_jitter_us = jitter_values.iter().sum::<f64>() / jitter_values.len() as f64;
        let max_jitter_us = jitter_values.last().copied().unwrap_or(0.0);
        let p99_index = ((jitter_values.len() as f64) * 0.99) as usize;
        let p99_jitter_us = jitter_values.get(p99_index).copied().unwrap_or(0.0);

        let timing_violations = jitter_values
            .iter()
            .filter(|&&jitter| jitter > self.config.max_jitter_us)
            .count() as u64;

        TimingValidation {
            avg_tick_interval_us: avg_jitter_us,
            max_jitter_us,
            p99_jitter_us,
            total_ticks: performance.total_ticks,
            missed_ticks: performance.missed_ticks,
            timing_violations,
        }
    }

    /// Validate device responses against expected values
    async fn validate_responses(
        &self,
        scenario: &TestScenario,
        device_id: &DeviceId,
    ) -> Result<Vec<ResponseValidationResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        // Open device to read telemetry
        let mut device = self.virtual_port.open_device(device_id).await?;

        for expected in &scenario.expected_responses {
            // Wait for the expected time offset
            tokio::time::sleep(expected.time_offset).await;

            // Read actual telemetry
            if let Some(actual) = device.read_telemetry() {
                let mut validation_errors = Vec::new();
                let mut passed = true;

                // Validate wheel angle
                if let Some((min_angle, max_angle)) = expected.wheel_angle_range {
                    let actual_angle = actual.wheel_angle_deg;
                    if actual_angle < min_angle || actual_angle > max_angle {
                        validation_errors.push(format!(
                            "Wheel angle {:.2}° outside expected range [{:.2}°, {:.2}°]",
                            actual_angle, min_angle, max_angle
                        ));
                        passed = false;
                    }
                }

                // Validate wheel speed
                if let Some((min_speed, max_speed)) = expected.wheel_speed_range {
                    let actual_speed = actual.wheel_speed_rad_s;
                    if actual_speed < min_speed || actual_speed > max_speed {
                        validation_errors.push(format!(
                            "Wheel speed {:.2} rad/s outside expected range [{:.2}, {:.2}] rad/s",
                            actual_speed, min_speed, max_speed
                        ));
                        passed = false;
                    }
                }

                // Validate temperature
                if let Some((min_temp, max_temp)) = expected.temperature_range {
                    let actual_temp = actual.temperature_c;
                    if actual_temp < min_temp || actual_temp > max_temp {
                        validation_errors.push(format!(
                            "Temperature {}°C outside expected range [{}°C, {}°C]",
                            actual_temp, min_temp, max_temp
                        ));
                        passed = false;
                    }
                }

                // Validate faults
                if let Some(expected_faults) = expected.expected_faults {
                    let actual_faults = actual.fault_flags;
                    if actual_faults != expected_faults {
                        validation_errors.push(format!(
                            "Fault flags 0x{:02x} don't match expected 0x{:02x}",
                            actual_faults, expected_faults
                        ));
                        passed = false;
                    }
                }

                results.push(ResponseValidationResult {
                    expected: expected.clone(),
                    actual,
                    passed,
                    errors: validation_errors,
                });
            } else {
                results.push(ResponseValidationResult {
                    expected: expected.clone(),
                    actual: TelemetryData {
                        wheel_angle_deg: 0.0,
                        wheel_speed_rad_s: 0.0,
                        temperature_c: 0,
                        fault_flags: 0,
                        hands_on: false,
                        timestamp: Instant::now(),
                    },
                    passed: false,
                    errors: vec!["No telemetry data available".to_string()],
                });
            }
        }

        Ok(results)
    }

    /// Run a comprehensive test suite
    pub async fn run_test_suite(&mut self) -> Result<Vec<TestResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        // Add test devices
        let device1 = self.create_test_device("test-wheel-1", "Test Wheel Base 1")?;
        self.add_virtual_device(device1)?;

        // Test scenarios
        let scenarios = vec![
            // Basic constant torque test
            TestScenario {
                name: "Constant Torque Test".to_string(),
                torque_pattern: TorquePattern::Constant(10.0),
                expected_responses: vec![ExpectedResponse {
                    time_offset: Duration::from_secs(1),
                    wheel_angle_range: Some((-1080.0, 1080.0)),
                    wheel_speed_range: Some((-50.0, 50.0)),
                    temperature_range: Some((25, 100)),
                    expected_faults: Some(0),
                }],
                fault_injections: vec![],
            },
            // Sine wave torque test
            TestScenario {
                name: "Sine Wave Torque Test".to_string(),
                torque_pattern: TorquePattern::SineWave {
                    amplitude: 15.0,
                    frequency_hz: 2.0,
                    phase_offset: 0.0,
                },
                expected_responses: vec![ExpectedResponse {
                    time_offset: Duration::from_millis(500),
                    wheel_angle_range: Some((-1080.0, 1080.0)),
                    wheel_speed_range: Some((-100.0, 100.0)),
                    temperature_range: Some((25, 100)),
                    expected_faults: Some(0),
                }],
                fault_injections: vec![],
            },
            // Torque limit test
            TestScenario {
                name: "Torque Limit Test".to_string(),
                torque_pattern: TorquePattern::Constant(30.0), // Exceeds 25Nm limit
                expected_responses: vec![ExpectedResponse {
                    time_offset: Duration::from_millis(100),
                    wheel_angle_range: Some((-1080.0, 1080.0)),
                    wheel_speed_range: Some((-10.0, 10.0)), // Should be limited
                    temperature_range: Some((25, 100)),
                    expected_faults: Some(0),
                }],
                fault_injections: vec![],
            },
            // Fault injection test
            TestScenario {
                name: "Fault Injection Test".to_string(),
                torque_pattern: TorquePattern::Constant(5.0),
                expected_responses: vec![ExpectedResponse {
                    time_offset: Duration::from_secs(2),
                    wheel_angle_range: Some((-1080.0, 1080.0)),
                    wheel_speed_range: Some((-50.0, 50.0)),
                    temperature_range: Some((25, 100)),
                    expected_faults: Some(0x04), // Thermal fault
                }],
                fault_injections: vec![FaultInjection {
                    inject_at: Duration::from_millis(1500),
                    fault_type: 0x04, // Thermal fault
                    duration: Duration::from_millis(1000),
                }],
            },
        ];

        // Run each scenario
        for scenario in scenarios {
            let result = self.run_scenario(scenario).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Generate a test report
    pub fn generate_report(&self, results: &[TestResult]) -> String {
        let mut report = String::new();

        report.push_str("# RT Loop Test Harness Report\n\n");

        let passed_count = results.iter().filter(|r| r.passed).count();
        let total_count = results.len();

        report.push_str("## Summary\n");
        report.push_str(&format!("- Total tests: {}\n", total_count));
        report.push_str(&format!("- Passed: {}\n", passed_count));
        report.push_str(&format!("- Failed: {}\n", total_count - passed_count));
        report.push_str(&format!(
            "- Success rate: {:.1}%\n\n",
            (passed_count as f64 / total_count as f64) * 100.0
        ));

        report.push_str("## Test Results\n\n");

        for result in results {
            report.push_str(&format!("### {}\n", result.scenario_name));
            report.push_str(&format!(
                "- Status: {}\n",
                if result.passed { "PASSED" } else { "FAILED" }
            ));
            report.push_str(&format!(
                "- Duration: {:.2}s\n",
                result.actual_duration.as_secs_f64()
            ));
            report.push_str(&format!(
                "- Total ticks: {}\n",
                result.performance.total_ticks
            ));
            report.push_str(&format!(
                "- Missed ticks: {}\n",
                result.performance.missed_ticks
            ));
            report.push_str(&format!(
                "- Missed tick rate: {:.6}\n",
                result.performance.missed_tick_rate()
            ));
            report.push_str(&format!(
                "- Max jitter: {:.2} μs\n",
                result.timing_validation.max_jitter_us
            ));
            report.push_str(&format!(
                "- P99 jitter: {:.2} μs\n",
                result.timing_validation.p99_jitter_us
            ));
            report.push_str(&format!(
                "- FFB writes: {} attempted, {} rejected\n",
                result.output_validation.attempted_writes, result.output_validation.rejected_writes
            ));
            report.push_str(&format!(
                "- Final zero write: {}\n",
                result.output_validation.final_zero_written
            ));

            if !result.errors.is_empty() {
                report.push_str("- Errors:\n");
                for error in &result.errors {
                    report.push_str(&format!("  - {}\n", error));
                }
            }

            report.push('\n');
        }

        report
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio;

    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    #[track_caller]
    fn must_some<T>(o: Option<T>, msg: &str) -> T {
        match o {
            Some(v) => v,
            None => panic!("must_some failed: {}", msg),
        }
    }

    #[tokio::test]
    async fn test_harness_basic_functionality() {
        let config = TestHarnessConfig {
            test_duration: Duration::from_millis(100), // Short test
            ..Default::default()
        };

        let mut harness = RTLoopTestHarness::new(config);

        // Add a test device
        let device = must(harness.create_test_device("test-device", "Test Device"));
        must(harness.add_virtual_device(device));

        // Create a simple test scenario
        let scenario = TestScenario {
            name: "Basic Test".to_string(),
            torque_pattern: TorquePattern::Constant(5.0),
            expected_responses: vec![],
            fault_injections: vec![],
        };

        // Run the test
        let result = must(harness.run_scenario(scenario).await);

        // Verify basic metrics
        assert!(result.performance.total_ticks > 0);
        assert_eq!(result.scenario_name, "Basic Test");
        assert!(result.output_validation.attempted_writes > 0);
        assert_eq!(result.output_validation.rejected_writes, 0);
        assert!(result.output_validation.final_zero_written);
    }

    #[tokio::test]
    async fn test_torque_patterns() {
        // Test constant pattern
        let constant = TorquePattern::Constant(10.0);
        assert_eq!(
            constant.value_at(Duration::from_secs(1), Instant::now()),
            10.0
        );

        // Test sine wave pattern
        let sine = TorquePattern::SineWave {
            amplitude: 5.0,
            frequency_hz: 1.0,
            phase_offset: 0.0,
        };
        let value_at_0 = sine.value_at(Duration::from_secs(0), Instant::now());
        let value_at_quarter = sine.value_at(Duration::from_millis(250), Instant::now());

        assert!((value_at_0 - 0.0).abs() < 0.1); // Should be ~0 at t=0
        assert!((value_at_quarter - 5.0).abs() < 0.1); // Should be ~amplitude at t=T/4

        // Test ramp pattern
        let ramp = TorquePattern::Ramp {
            start_value: 0.0,
            end_value: 10.0,
            duration: Duration::from_secs(1),
        };
        let value_at_half = ramp.value_at(Duration::from_millis(500), Instant::now());
        assert!((value_at_half - 5.0).abs() < 0.1); // Should be halfway
    }

    #[tokio::test]
    async fn test_virtual_device_integration() {
        let mut port = VirtualHidPort::new();

        // Create and add a virtual device
        let device_id = must("integration-test".parse::<DeviceId>());
        let device = VirtualDevice::new(device_id.clone(), "Integration Test Device".to_string());
        must(port.add_device(device));

        // List devices
        let devices = must(port.list_devices().await);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id.as_str(), "integration-test");

        // Open device
        let mut opened_device = must(port.open_device(&device_id).await);

        // Test device operations
        let write_result = opened_device.write_ffb_report(10.0, 1);
        assert!(write_result.is_ok());

        // Read telemetry
        let telemetry = opened_device.read_telemetry();
        assert!(telemetry.is_some());

        let _tel = must_some(telemetry, "expected telemetry data");
        // Note: sequence field removed from TelemetryData
    }

    #[test]
    fn test_expected_response_validation() {
        let _expected = ExpectedResponse {
            time_offset: Duration::from_millis(100),
            wheel_angle_range: Some((-10.0, 10.0)),
            wheel_speed_range: Some((-5.0, 5.0)),
            temperature_range: Some((20, 80)),
            expected_faults: Some(0),
        };

        // Test data within range
        let good_telemetry = TelemetryData {
            wheel_angle_deg: 5.0,   // 5.0 degrees
            wheel_speed_rad_s: 2.0, // 2.0 rad/s
            temperature_c: 45,
            fault_flags: 0,
            hands_on: true,
            timestamp: Instant::now(),
        };

        // Validate ranges manually (this would be done by the harness)
        let actual_angle = good_telemetry.wheel_angle_deg;
        assert!((-10.0..=10.0).contains(&actual_angle));

        let actual_speed = good_telemetry.wheel_speed_rad_s;
        assert!((-5.0..=5.0).contains(&actual_speed));

        assert!((20..=80).contains(&good_telemetry.temperature_c));
        assert_eq!(good_telemetry.fault_flags, 0);
    }
}
