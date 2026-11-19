#[napi_derive::napi(object)]
#[derive(Debug)]
pub struct BindingNativePluginConfig {
  pub source: String,
  pub config: Option<serde_json::Value>,
}
