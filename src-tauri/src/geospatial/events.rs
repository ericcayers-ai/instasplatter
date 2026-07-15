//! Geospatial job / suite events emitted to the frontend.

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
    },
    #[serde(rename_all = "camelCase")]
    Error {
        message: String,
    },
}

pub const GEO_EVENT_CHANNEL: &str = "geo://event";
