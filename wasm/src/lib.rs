//! Thin wasm-bindgen shim over `blue_marshal`'s decode/encode + JSON
//! conversion, for use from a browser (see `../site/`).

use wasm_bindgen::prelude::*;

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Decode a `.marshal`/EVE configuration file and return its lossless JSON
/// representation as a pretty-printed string.
#[wasm_bindgen]
pub fn decode_to_json(bytes: &[u8]) -> Result<String, JsValue> {
    let decoded = blue_marshal::decode(bytes).map_err(js_err)?;
    let json = blue_marshal::to_json(&decoded.value);
    serde_json::to_string_pretty(&json).map_err(js_err)
}

/// Encode the lossless JSON representation (as produced by
/// `decode_to_json`, possibly edited) back into marshal binary bytes.
#[wasm_bindgen]
pub fn encode_from_json(json_text: &str) -> Result<Vec<u8>, JsValue> {
    let json: serde_json::Value = serde_json::from_str(json_text).map_err(js_err)?;
    let value = blue_marshal::from_json(&json).map_err(js_err)?;
    blue_marshal::encode(&value, &blue_marshal::EncodeOptions::default()).map_err(js_err)
}
