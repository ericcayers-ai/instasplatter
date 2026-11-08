//! Geospatial job / suite events emitted to the frontend.
//!
//! `geo://event` covers catalog/layer/run lifecycle. `sim://event` streams
//! simulation checkpoints for both the scientific ANUGA path and the WebGPU
//! live-preview agent (shared protocol — do not diverge casually).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GeoEvent {
    #[serde(rename_all = "camelCase")]
    LayerAdded {
        workspace: String,
        layer_id: String,
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    ScenarioUpdated {
        workspace: String,
        scenario_id: String,
    },
    #[serde(rename_all = "camelCase")]
    RunProgress {
        run_id: String,
        progress: f32,
        detail: String,
    },
    #[serde(rename_all = "camelCase")]
    RunDone {
        run_id: String,
        result_paths: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mass_balance: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    RunCancelled { run_id: String },
    #[serde(rename_all = "camelCase")]
    EngineMissing {
        engine: String,
        message: String,
        demo_available: bool,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
    },
}

/// Progressive simulation frames shared by scientific + preview engines.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SimEvent {
    #[serde(rename_all = "camelCase")]
    Checkpoint {
        run_id: String,
        progress: f32,
        sim_time_hours: f64,
        checkpoint_path: Option<String>,
        detail: String,
        /// `"anuga"` | `"demo"` | `"preview"` | …
        mode: String,
        /// Optional stats for live-preview vs scientific comparison.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_depth_m: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        wet_fraction: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mass_m3: Option<f32>,
    },
    #[serde(rename_all = "camelCase")]
    Hydrograph {
        run_id: String,
        path: String,
        samples: Vec<HydrographSample>,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        run_id: String,
        mode: String,
        result_paths: Vec<String>,
        mass_balance: Option<f64>,
        label: Option<String>,
    },
}

/// A discharge / water-stage sample attached to a simulation hydrograph event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HydrographSample {
    pub hours: f64,
    pub stage_m: f64,
    pub discharge_cms: f64,
}

#[cfg(test)]
mod tests {
    use super::{HydrographSample, SimEvent};

    #[test]
    fn hydrograph_event_serializes_typed_samples_in_camel_case() {
        let event = SimEvent::Hydrograph {
            run_id: "run-1".into(),
            path: "hydrograph.json".into(),
            samples: vec![HydrographSample {
                hours: 2.5,
                stage_m: 1.25,
                discharge_cms: 18.0,
            }],
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["kind"], "hydrograph");
        assert_eq!(value["runId"], "run-1");
        assert_eq!(value["path"], "hydrograph.json");
        assert_eq!(
            value["samples"][0],
            serde_json::json!({
                "hours": 2.5,
                "stageM": 1.25,
                "dischargeCms": 18.0
            })
        );
    }
}

pub const GEO_EVENT_CHANNEL: &str = "geo://event";
pub const SIM_EVENT_CHANNEL: &str = "sim://event";
