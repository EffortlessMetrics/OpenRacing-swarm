//! Example demonstrating service layer usage of IPC conversion layer
//!
//! This example shows how a service implementation would use the conversion
//! layer to separate domain logic from wire protocol concerns.

#[cfg(test)]
mod example {
    use crate::domain::*;
    use crate::entities::*;
    use crate::generated::wheel::v1 as proto;
    use crate::telemetry::TelemetryData;
    use std::collections::HashMap;
    use std::f64::consts::E;

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must failed: {:?}", e),
        }
    }

    fn must_some<T>(o: Option<T>, msg: &str) -> T {
        match o {
            Some(v) => v,
            None => panic!("must_some failed: {}", msg),
        }
    }

    /// Mock device service that works purely with domain types
    struct MockDeviceService {
        devices: HashMap<DeviceId, Device>,
        telemetry: HashMap<DeviceId, TelemetryData>,
    }

    impl MockDeviceService {
        fn new() -> Self {
            let mut service = Self {
                devices: HashMap::new(),
                telemetry: HashMap::new(),
            };

            // Add some mock devices
            let device_id: DeviceId = must("csl-dd-001".parse());
            let capabilities = DeviceCapabilities::new(
                true,
                true,
                true,
                false,
                must(TorqueNm::new(8.0)),
                32768, // 2^15, within u16 range
                1000,
            );

            let device = Device::new(
                device_id.clone(),
                "Fanatec CSL DD 8Nm".to_string(),
                DeviceType::WheelBase,
                capabilities,
            );

            let telemetry = TelemetryData {
                wheel_angle_deg: 0.0,
                wheel_speed_rad_s: 0.0,
                temperature_c: 35,
                fault_flags: 0,
                hands_on: false,
                timestamp: 0,
            };

            service.devices.insert(device_id.clone(), device);
            service.telemetry.insert(device_id, telemetry);

            service
        }

        /// List all devices (returns domain types)
        fn list_devices(&self) -> Vec<Device> {
            self.devices.values().cloned().collect()
        }

        /// Get device status (returns domain types)
        fn get_device_status(&self, device_id: &DeviceId) -> Option<(Device, TelemetryData)> {
            let device = self.devices.get(device_id)?;
            let telemetry = self.telemetry.get(device_id)?;
            Some((device.clone(), telemetry.clone()))
        }

        /// Update telemetry (accepts domain types)
        fn update_telemetry(&mut self, device_id: &DeviceId, telemetry: TelemetryData) {
            self.telemetry.insert(device_id.clone(), telemetry);
        }
    }

    /// Mock IPC service that handles wire protocol conversion
    struct MockIpcService {
        device_service: MockDeviceService,
    }

    impl MockIpcService {
        fn new() -> Self {
            Self {
                device_service: MockDeviceService::new(),
            }
        }

        /// IPC method: List devices (converts domain -> wire)
        fn list_devices_ipc(&self) -> Vec<proto::DeviceInfo> {
            // Service layer returns domain types
            let devices = self.device_service.list_devices();

            // IPC layer converts to wire types
            devices.into_iter().map(Into::into).collect()
        }

        /// IPC method: Get device status (converts domain -> wire)
        fn get_device_status_ipc(
            &self,
            wire_device_id: proto::DeviceId,
        ) -> Option<proto::DeviceStatus> {
            // Convert wire type to domain type
            let device_id: DeviceId = wire_device_id.id.parse().ok()?;

            // Service layer works with domain types
            let (device, telemetry) = self.device_service.get_device_status(&device_id)?;

            // Convert domain types to wire types
            let wire_device: proto::DeviceInfo = device.into();
            let wire_telemetry: proto::TelemetryData = telemetry.into();

            Some(proto::DeviceStatus {
                device: Some(wire_device),
                last_seen: Some(prost_types::Timestamp {
                    seconds: 1234567890,
                    nanos: 0,
                }),
                active_faults: vec![],
                telemetry: Some(wire_telemetry),
                moza: None,
            })
        }

        /// IPC method: Update telemetry (converts wire -> domain)
        fn update_telemetry_ipc(
            &mut self,
            wire_device_id: proto::DeviceId,
            wire_telemetry: proto::TelemetryData,
        ) -> Result<(), String> {
            // Convert wire types to domain types
            let device_id: DeviceId = wire_device_id
                .id
                .parse()
                .map_err(|e| format!("Invalid device ID: {}", e))?;

            let telemetry: TelemetryData = wire_telemetry
                .try_into()
                .map_err(|e| format!("Invalid telemetry: {}", e))?;

            // Service layer works with domain types
            self.device_service.update_telemetry(&device_id, telemetry);

            Ok(())
        }
    }

    #[test]
    fn test_service_example() {
        let mut ipc_service = MockIpcService::new();

        // Test listing devices
        let wire_devices = ipc_service.list_devices_ipc();
        assert_eq!(wire_devices.len(), 1);
        assert_eq!(wire_devices[0].id, "csl-dd-001");
        assert_eq!(wire_devices[0].name, "Fanatec CSL DD 8Nm");
        assert_eq!(wire_devices[0].r#type, 1); // WheelBase

        // Test getting device status
        let wire_device_id = proto::DeviceId {
            id: "csl-dd-001".to_string(),
        };

        let status = must_some(
            ipc_service.get_device_status_ipc(wire_device_id),
            "expected device status",
        );
        let device_info = must_some(status.device, "expected device info");
        let telemetry = must_some(status.telemetry, "expected telemetry");

        assert_eq!(device_info.id, "csl-dd-001");
        assert_eq!(telemetry.temp_c, 35);
        assert_eq!(telemetry.wheel_angle_mdeg, 0);

        // Test updating telemetry
        let new_telemetry = proto::TelemetryData {
            wheel_angle_mdeg: 90000,  // 90 degrees
            wheel_speed_mrad_s: 1570, // ~1.57 rad/s
            temp_c: 45,
            faults: 0,
            hands_on: true,
            sequence: 0,
        };

        let device_id = proto::DeviceId {
            id: "csl-dd-001".to_string(),
        };

        let result = ipc_service.update_telemetry_ipc(device_id.clone(), new_telemetry);
        assert!(result.is_ok());

        // Verify the update worked
        let updated_status = must_some(
            ipc_service.get_device_status_ipc(device_id),
            "expected device status",
        );
        let updated_telemetry = must_some(updated_status.telemetry, "expected telemetry");

        assert_eq!(updated_telemetry.wheel_angle_mdeg, 90000);
        assert_eq!(updated_telemetry.wheel_speed_mrad_s, 1570);
        assert_eq!(updated_telemetry.temp_c, 45);
        assert!(updated_telemetry.hands_on);
    }

    #[test]
    fn test_validation_in_service() {
        let mut ipc_service = MockIpcService::new();

        // Test that invalid telemetry is rejected
        let invalid_telemetry = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 200, // Invalid: > 150°C
            faults: 0,
            hands_on: false,
            sequence: 0,
        };

        let device_id = proto::DeviceId {
            id: "csl-dd-001".to_string(),
        };

        let result = ipc_service.update_telemetry_ipc(device_id, invalid_telemetry);
        assert!(result.is_err());
        assert!(must_some(result.err(), "expected error").contains("Invalid telemetry"));

        // Test that invalid device ID is rejected
        let valid_telemetry = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 50,
            faults: 0,
            hands_on: false,
            sequence: 0,
        };

        let invalid_device_id = proto::DeviceId {
            id: "".to_string(), // Empty device ID is invalid
        };

        let result = ipc_service.update_telemetry_ipc(invalid_device_id, valid_telemetry);
        assert!(result.is_err());
        assert!(must_some(result.err(), "expected error").contains("Invalid device ID"));
    }

    #[test]
    fn test_unit_conversion_accuracy() {
        let _ipc_service = MockIpcService::new();

        // Test precise unit conversions
        let test_cases = vec![
            (123.456, 123456), // degrees to millidegrees
            (E, 2718),         // rad/s to mrad/s
            (0.001, 1),        // small values
            (999.999, 999999), // large values
        ];

        for (domain_value, expected_wire_value) in test_cases {
            let telemetry = TelemetryData {
                wheel_angle_deg: domain_value as f32,
                wheel_speed_rad_s: domain_value as f32,
                temperature_c: 50,
                fault_flags: 0,
                hands_on: true,
                timestamp: 1000,
            };

            // Convert to wire format
            let wire_telemetry: proto::TelemetryData = telemetry.into();

            // Check unit conversion
            assert_eq!(wire_telemetry.wheel_angle_mdeg, expected_wire_value);
            assert_eq!(wire_telemetry.wheel_speed_mrad_s, expected_wire_value);

            // Convert back to domain format
            let back_to_domain: TelemetryData = must(wire_telemetry.try_into());

            // Check precision preservation (within 0.001 tolerance)
            assert!((back_to_domain.wheel_angle_deg - domain_value as f32).abs() < 0.001);
            assert!((back_to_domain.wheel_speed_rad_s - domain_value as f32).abs() < 0.001);
        }
    }
}
