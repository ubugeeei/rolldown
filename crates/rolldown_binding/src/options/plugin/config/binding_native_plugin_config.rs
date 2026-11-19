#[napi_derive::napi(object)]
#[derive(Debug)]
pub struct BindingNativePluginConfig {
  pub library: String,
  pub config: Option<String>,
}
