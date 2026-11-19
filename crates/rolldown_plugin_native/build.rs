fn main() {
  println!(
    "cargo:rustc-env=ROLLDOWN_NATIVE_PLUGIN_TARGET={}",
    std::env::var("TARGET").expect("Cargo must provide TARGET")
  );
}
