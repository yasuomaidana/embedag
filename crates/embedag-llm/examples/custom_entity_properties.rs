//! Developer-defined custom entity properties example.
//!
//! Demonstrates how developers define custom entity properties and extraction schemas
//! using the Diesel-inspired [`entity_properties!`] macro, without relying on hardcoded
//! application or domain-specific keys in the core library.

use embedag_llm::entity_properties;

// 1. Healthcare EHR domain schema: developer specifies patient identifier keys
entity_properties! {
    PatientRecord {
        keys: ["mrn", "patient_id", "national_health_id"],
        default_label: "Anonymous Patient",
    }
}

// 2. Industrial IoT / Telemetry domain schema: developer specifies sensor hardware keys
entity_properties! {
    SensorDeviceRecord {
        keys: ["mac_address", "sensor_uuid", "device_serial"],
        default_label: "Unknown Telemetry Node",
    }
}

fn main() {
    println!("=== Embedag: Developer-Defined Entity Properties (Diesel Macro Concept) ===\n");

    // Healthcare EHR extraction test
    let ehr_prompt = r#"### INCOMING RECORD
{"mrn": "MRN-2026-90412", "department": "Cardiology", "ward": "ICU-3"}

### CANDIDATE PARENT
{"facility_id": "ST-MARYS-MAIN"}"#;

    let patient_extractor = PatientRecord;
    let patient_label = patient_extractor.extract_from_prompt(ehr_prompt);
    println!(
        "Extracted Healthcare Patient Identity: \"{}\"",
        patient_label
    );
    assert_eq!(patient_label, "MRN-2026-90412");

    // IoT Telemetry extraction test
    let iot_prompt = r#"### INCOMING RECORD
{"mac_address": "00:1B:44:11:3A:B7", "firmware_version": "v3.1.2", "voltage": 3.3}

### CANDIDATE PARENT
{"gateway_id": "GATEWAY-EU-CENTRAL"}"#;

    let sensor_extractor = SensorDeviceRecord;
    let sensor_label = sensor_extractor.extract_from_prompt(iot_prompt);
    println!("Extracted IoT Hardware Identity: \"{}\"", sensor_label);
    assert_eq!(sensor_label, "00:1B:44:11:3A:B7");

    println!("\nSUCCESS: All developer-defined schemas resolved accurately without hardcoding.");
}
