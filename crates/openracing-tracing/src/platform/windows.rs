//! Windows ETW (Event Tracing for Windows) provider

use crate::{AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics, TracingProvider};
use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::System::Diagnostics::Etw::{
    EVENT_DESCRIPTOR, EventRegister, EventUnregister, EventWrite, REGHANDLE,
};
use windows::core::GUID;

/// ETW Provider GUID for OpenRacing
const PROVIDER_GUID: u128 = 0x12345678_1234_5678_9ABC_123456789ABC;

/// Windows ETW provider implementation
///
/// Uses Event Tracing for Windows for high-performance, RT-safe tracing.
///
/// # RT Safety
///
/// ETW is RT-safe:
/// - EventWrite is designed for kernel-mode use
/// - No allocations in the hot path
/// - Bounded execution time
/// - Can be enabled/disabled dynamically via ETW sessions
pub struct WindowsETWProvider {
    provider_handle: Option<REGHANDLE>,
    rt_events_count: AtomicU64,
    app_events_count: AtomicU64,
}

impl WindowsETWProvider {
    /// Create a new ETW provider
    pub fn new() -> Result<Self, TracingError> {
        Ok(Self {
            provider_handle: None,
            rt_events_count: AtomicU64::new(0),
            app_events_count: AtomicU64::new(0),
        })
    }

    fn emit_etw_event(&self, handle: REGHANDLE, event: RTTraceEvent) {
        let event_descriptor = rt_event_descriptor(event);
        write_etw_event(handle, &event_descriptor);
        self.rt_events_count.fetch_add(1, Ordering::Relaxed);
    }

    fn emit_etw_app_event(&self, handle: REGHANDLE, _event: &AppTraceEvent) {
        let event_descriptor = app_event_descriptor();
        write_etw_event(handle, &event_descriptor);
        self.app_events_count.fetch_add(1, Ordering::Relaxed);
    }
}

fn rt_event_descriptor(event: RTTraceEvent) -> EVENT_DESCRIPTOR {
    match event {
        RTTraceEvent::TickStart { .. } => EVENT_DESCRIPTOR {
            Id: 1,
            Version: 1,
            Channel: 0,
            Level: 4,
            Opcode: 1,
            Task: 1,
            Keyword: 0x1,
        },
        RTTraceEvent::TickEnd { .. } => EVENT_DESCRIPTOR {
            Id: 2,
            Version: 1,
            Channel: 0,
            Level: 4,
            Opcode: 2,
            Task: 1,
            Keyword: 0x1,
        },
        RTTraceEvent::HidWrite { .. } => EVENT_DESCRIPTOR {
            Id: 3,
            Version: 1,
            Channel: 0,
            Level: 4,
            Opcode: 0,
            Task: 2,
            Keyword: 0x2,
        },
        RTTraceEvent::DeadlineMiss { .. } => EVENT_DESCRIPTOR {
            Id: 4,
            Version: 1,
            Channel: 0,
            Level: 2,
            Opcode: 0,
            Task: 1,
            Keyword: 0x4,
        },
        RTTraceEvent::PipelineFault { .. } => EVENT_DESCRIPTOR {
            Id: 5,
            Version: 1,
            Channel: 0,
            Level: 1,
            Opcode: 0,
            Task: 3,
            Keyword: 0x4,
        },
    }
}

fn app_event_descriptor() -> EVENT_DESCRIPTOR {
    EVENT_DESCRIPTOR {
        Id: 100,
        Version: 1,
        Channel: 0,
        Level: 4,
        Opcode: 0,
        Task: 10,
        Keyword: 0x10,
    }
}

fn descriptor_is_reviewed_for_etw(descriptor: &EVENT_DESCRIPTOR) -> bool {
    descriptor.Id != 0
        && descriptor.Version == 1
        && (1..=5).contains(&descriptor.Level)
        && descriptor.Task != 0
        && descriptor.Keyword != 0
}

fn registered_etw_handle(handle: REGHANDLE) -> bool {
    handle.0 != 0
}

fn register_etw_provider(provider_guid: &GUID) -> Result<REGHANDLE, TracingError> {
    let mut handle = REGHANDLE(0);

    // SAFETY: `provider_guid` is a valid immutable GUID for the duration of
    // the call, callback/context pointers are intentionally absent, and
    // `handle` is a valid out-parameter owned by this stack frame.
    let result = unsafe { EventRegister(provider_guid, None, None, &mut handle) };

    if result != 0 {
        return Err(TracingError::InitializationFailed(format!(
            "EventRegister failed with code: {}",
            result
        )));
    }

    if !registered_etw_handle(handle) {
        return Err(TracingError::InitializationFailed(
            "EventRegister returned a zero provider handle".to_string(),
        ));
    }

    Ok(handle)
}

fn write_etw_event(handle: REGHANDLE, descriptor: &EVENT_DESCRIPTOR) {
    if !registered_etw_handle(handle) || !descriptor_is_reviewed_for_etw(descriptor) {
        return;
    }

    // SAFETY: `handle` passed the nonzero registration guard, `descriptor`
    // passed the local ETW descriptor contract, and the user-data argument is
    // `None`, so no payload pointer or length pair is provided to ETW.
    unsafe {
        let _ = EventWrite(handle, descriptor, None);
    }
}

fn unregister_etw_provider(handle: REGHANDLE) {
    if !registered_etw_handle(handle) {
        return;
    }

    // SAFETY: only nonzero handles previously stored by successful
    // `EventRegister` are passed here, and the provider handle is removed from
    // `WindowsETWProvider` before this call so it is not unregistered twice.
    unsafe {
        let _ = EventUnregister(handle);
    }
}

impl TracingProvider for WindowsETWProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        let provider_guid = GUID::from_u128(PROVIDER_GUID);
        let handle = register_etw_provider(&provider_guid)?;

        self.provider_handle = Some(handle);
        tracing::info!("ETW provider initialized with handle: {}", handle.0);
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Some(handle) = self.provider_handle {
            self.emit_etw_event(handle, event);
        }
    }

    fn emit_app_event(&self, event: AppTraceEvent) {
        match &event {
            AppTraceEvent::DeviceConnected {
                device_id,
                device_name,
                capabilities,
            } => {
                tracing::info!(
                    device_id = %device_id,
                    device_name = %device_name,
                    capabilities = %capabilities,
                    "Device connected"
                );
            }
            AppTraceEvent::DeviceDisconnected { device_id, reason } => {
                tracing::warn!(
                    device_id = %device_id,
                    reason = %reason,
                    "Device disconnected"
                );
            }
            AppTraceEvent::TelemetryStarted {
                game_id,
                telemetry_rate_hz,
            } => {
                tracing::info!(
                    game_id = %game_id,
                    telemetry_rate_hz = %telemetry_rate_hz,
                    "Telemetry started"
                );
            }
            AppTraceEvent::ProfileApplied {
                device_id,
                profile_name,
                profile_hash,
            } => {
                tracing::info!(
                    device_id = %device_id,
                    profile_name = %profile_name,
                    profile_hash = %profile_hash,
                    "Profile applied"
                );
            }
            AppTraceEvent::SafetyStateChanged {
                device_id,
                old_state,
                new_state,
                reason,
            } => {
                tracing::warn!(
                    device_id = %device_id,
                    old_state = %old_state,
                    new_state = %new_state,
                    reason = %reason,
                    "Safety state changed"
                );
            }
        }

        if let Some(handle) = self.provider_handle {
            self.emit_etw_app_event(handle, &event);
        }
    }

    fn metrics(&self) -> TracingMetrics {
        TracingMetrics {
            rt_events_emitted: self.rt_events_count.load(Ordering::Relaxed),
            app_events_emitted: self.app_events_count.load(Ordering::Relaxed),
            ..Default::default()
        }
    }

    fn is_enabled(&self) -> bool {
        self.provider_handle.is_some()
    }

    fn shutdown(&mut self) {
        if let Some(handle) = self.provider_handle.take() {
            unregister_etw_provider(handle);
            tracing::info!("ETW provider shutdown");
        }
    }
}

impl core::fmt::Debug for WindowsETWProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WindowsETWProvider")
            .field("provider_handle", &self.provider_handle.map(|h| h.0))
            .field(
                "rt_events_count",
                &self.rt_events_count.load(Ordering::Relaxed),
            )
            .field(
                "app_events_count",
                &self.app_events_count.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl Default for WindowsETWProvider {
    fn default() -> Self {
        // WindowsETWProvider::new() is infallible in practice
        Self::new().unwrap_or_else(|_| Self {
            provider_handle: None,
            rt_events_count: AtomicU64::new(0),
            app_events_count: AtomicU64::new(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etw_provider_creation() {
        let result = WindowsETWProvider::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_etw_provider_lifecycle() {
        let mut provider = WindowsETWProvider::new().expect("creation failed");

        let init_result = provider.initialize();
        if init_result.is_ok() {
            provider.emit_rt_event(RTTraceEvent::TickStart {
                tick_count: 1,
                timestamp_ns: 1000,
            });

            let metrics = provider.metrics();
            assert_eq!(metrics.rt_events_emitted, 1);

            provider.shutdown();
            assert!(!provider.is_enabled());
        }
    }

    #[test]
    fn test_etw_registered_handle_guard() {
        assert!(!registered_etw_handle(REGHANDLE(0)));
        assert!(registered_etw_handle(REGHANDLE(1)));
    }

    #[test]
    fn test_etw_app_descriptor_contract() {
        let descriptor = app_event_descriptor();
        assert!(descriptor_is_reviewed_for_etw(&descriptor));
        assert_eq!(descriptor.Id, 100);
        assert_eq!(descriptor.Task, 10);
        assert_eq!(descriptor.Keyword, 0x10);
    }

    #[test]
    fn test_etw_rt_descriptor_contracts() {
        let events = [
            RTTraceEvent::TickStart {
                tick_count: 1,
                timestamp_ns: 100,
            },
            RTTraceEvent::TickEnd {
                tick_count: 1,
                timestamp_ns: 200,
                processing_time_ns: 50,
            },
            RTTraceEvent::HidWrite {
                tick_count: 1,
                timestamp_ns: 300,
                torque_nm: 1.0,
                seq: 7,
            },
            RTTraceEvent::DeadlineMiss {
                tick_count: 1,
                timestamp_ns: 400,
                jitter_ns: 25,
            },
            RTTraceEvent::PipelineFault {
                tick_count: 1,
                timestamp_ns: 500,
                error_code: 9,
            },
        ];

        for (event, expected_id) in events.into_iter().zip(1..=5) {
            let descriptor = rt_event_descriptor(event);
            assert!(descriptor_is_reviewed_for_etw(&descriptor));
            assert_eq!(descriptor.Id, expected_id);
        }
    }

    #[test]
    fn test_etw_descriptor_contract_rejects_invalid_shapes() {
        let mut descriptor = app_event_descriptor();
        descriptor.Id = 0;
        assert!(!descriptor_is_reviewed_for_etw(&descriptor));

        descriptor = app_event_descriptor();
        descriptor.Level = 0;
        assert!(!descriptor_is_reviewed_for_etw(&descriptor));

        descriptor = app_event_descriptor();
        descriptor.Task = 0;
        assert!(!descriptor_is_reviewed_for_etw(&descriptor));

        descriptor = app_event_descriptor();
        descriptor.Keyword = 0;
        assert!(!descriptor_is_reviewed_for_etw(&descriptor));
    }
}
