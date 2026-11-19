use std::{
  collections::BTreeMap,
  fs,
  path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use oxc_resolver::{ResolveOptions, Resolver};
use rolldown_plugin_native_abi::ABI_VERSION;
use serde::Deserialize;

use super::with_platform_library_name;

pub const TARGET: &str = env!("ROLLDOWN_NATIVE_PLUGIN_TARGET");

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PackageManifest {
  rolldown: NativePluginManifest,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NativePluginManifest {
  abi: Option<u32>,
  artifacts: BTreeMap<String, String>,
  binary: Option<String>,
  dev_binary: Option<String>,
}

pub fn resolve_package_library(specifier: &str, from: &Path) -> Result<PathBuf> {
  validate_package_name(specifier)?;
  if !from.is_absolute() {
    bail!("native plugin package resolution requires an absolute cwd: {}", from.display());
  }

  let resolver = package_resolver(from);
  let package_dir = resolve_package_dir(&resolver, specifier, from)
    .with_context(|| format!("failed to resolve native plugin package {specifier:?}"))?;
  let package = read_package(&package_dir)?;
  validate_abi(specifier, &package)?;

  if let Some(dev_binary) = package.rolldown.dev_binary.as_deref() {
    return resolve_package_path(&package_dir, dev_binary, true).with_context(|| {
      format!("failed to resolve rolldown.devBinary for native plugin package {specifier:?}")
    });
  }

  let artifact = package.rolldown.artifacts.get(TARGET).ok_or_else(|| {
    anyhow::anyhow!("native plugin package {specifier:?} does not provide an artifact for {TARGET}")
  })?;

  if artifact.starts_with("./") {
    return resolve_package_path(&package_dir, artifact, false).with_context(|| {
      format!("failed to resolve the {TARGET} artifact for native plugin package {specifier:?}")
    });
  }

  validate_package_name(artifact).with_context(|| {
    format!("native plugin package {specifier:?} declares an invalid artifact package")
  })?;
  let artifact_dir = resolve_package_dir(&resolver, artifact, &package_dir).with_context(|| {
    format!(
      "native plugin package {specifier:?} is installed, but its {TARGET} artifact package \
       {artifact:?} is unavailable"
    )
  })?;
  let artifact_package = read_package(&artifact_dir)?;
  validate_abi(artifact, &artifact_package)?;
  let binary = artifact_package.rolldown.binary.as_deref().ok_or_else(|| {
    anyhow::anyhow!("artifact package {artifact:?} does not declare rolldown.binary")
  })?;

  resolve_package_path(&artifact_dir, binary, false)
    .with_context(|| format!("failed to resolve rolldown.binary in artifact package {artifact:?}"))
}

fn package_resolver(from: &Path) -> Resolver {
  let options = ResolveOptions {
    cwd: Some(from.to_path_buf()),
    resolve_to_context: true,
    symlinks: true,
    yarn_pnp: from.ancestors().any(|directory| directory.join(".pnp.cjs").is_file()),
    ..Default::default()
  };
  Resolver::new(options)
}

fn resolve_package_dir(resolver: &Resolver, specifier: &str, from: &Path) -> Result<PathBuf> {
  let path = resolver.resolve(from, specifier)?.into_path_buf();
  path
    .canonicalize()
    .with_context(|| format!("failed to canonicalize package directory {}", path.display()))
}

fn read_package(package_dir: &Path) -> Result<PackageManifest> {
  let path = package_dir.join("package.json");
  let source = fs::read_to_string(&path)
    .with_context(|| format!("failed to read native plugin manifest {}", path.display()))?;
  serde_json::from_str(&source)
    .with_context(|| format!("failed to parse native plugin manifest {}", path.display()))
}

fn validate_abi(specifier: &str, package: &PackageManifest) -> Result<()> {
  let Some(abi) = package.rolldown.abi else {
    bail!("native plugin package {specifier:?} does not declare rolldown.abi");
  };
  if abi != ABI_VERSION {
    bail!(
      "native plugin package {specifier:?} requires ABI {abi}, but this Rolldown supports ABI \
       {ABI_VERSION}"
    );
  }
  Ok(())
}

fn resolve_package_path(
  package_dir: &Path,
  value: &str,
  infer_platform_name: bool,
) -> Result<PathBuf> {
  let relative = Path::new(value);
  if relative.is_absolute()
    || relative.components().any(|component| {
      matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    })
  {
    bail!("native plugin package paths must stay inside the package: {value:?}");
  }

  let path = package_dir.join(relative);
  let path = if infer_platform_name { with_platform_library_name(path)? } else { path };
  let path = path
    .canonicalize()
    .with_context(|| format!("failed to resolve native plugin library {}", path.display()))?;
  if !path.starts_with(package_dir) {
    bail!("native plugin library resolves outside its package: {}", path.display());
  }
  Ok(path)
}

fn validate_package_name(specifier: &str) -> Result<()> {
  let valid = if let Some(scoped) = specifier.strip_prefix('@') {
    let mut parts = scoped.split('/');
    parts.next().is_some_and(|part| !part.is_empty())
      && parts.next().is_some_and(|part| !part.is_empty())
      && parts.next().is_none()
  } else {
    !specifier.is_empty() && !specifier.contains('/')
  };

  if !valid || specifier.starts_with('.') || specifier.contains(['\\', '\0', ':']) {
    bail!("native plugin source must be a package name, absolute path, or file URL");
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn write(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
  }

  fn platform_library_name(name: &str) -> String {
    format!("{}{name}{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX)
  }

  #[test]
  fn resolves_nested_platform_artifact_package() {
    let temp = tempfile::tempdir().unwrap();
    let plugin = temp.path().join("node_modules/@scope/plugin");
    let artifact_name = "@scope/plugin-platform";
    let artifact = plugin.join("node_modules/@scope/plugin-platform");
    write(
      &plugin.join("package.json"),
      &format!(
        r#"{{"name":"@scope/plugin","rolldown":{{"abi":{ABI_VERSION},"artifacts":{{"{TARGET}":"{artifact_name}"}}}}}}"#
      ),
    );
    write(
      &artifact.join("package.json"),
      &format!(
        r#"{{"name":"{artifact_name}","rolldown":{{"abi":{ABI_VERSION},"binary":"plugin{}"}}}}"#,
        std::env::consts::DLL_SUFFIX
      ),
    );
    let library = artifact.join(format!("plugin{}", std::env::consts::DLL_SUFFIX));
    write(&library, "");

    assert_eq!(
      resolve_package_library("@scope/plugin", temp.path()).unwrap(),
      library.canonicalize().unwrap()
    );
  }

  #[test]
  fn resolves_extensionless_development_library() {
    let temp = tempfile::tempdir().unwrap();
    let plugin = temp.path().join("node_modules/plugin");
    write(
      &plugin.join("package.json"),
      &format!(r#"{{"rolldown":{{"abi":{ABI_VERSION},"devBinary":"./target/plugin"}}}}"#),
    );
    let library = plugin.join("target").join(platform_library_name("plugin"));
    write(&library, "");

    assert_eq!(
      resolve_package_library("plugin", temp.path()).unwrap(),
      library.canonicalize().unwrap()
    );
  }

  #[test]
  fn rejects_missing_and_incompatible_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let plugin = temp.path().join("node_modules/plugin");
    write(&plugin.join("package.json"), r#"{"name":"plugin"}"#);
    assert!(
      resolve_package_library("plugin", temp.path())
        .unwrap_err()
        .to_string()
        .contains("rolldown.abi")
    );

    write(&plugin.join("package.json"), r#"{"rolldown":{"abi":999,"artifacts":{}}}"#);
    assert!(
      resolve_package_library("plugin", temp.path())
        .unwrap_err()
        .to_string()
        .contains("requires ABI 999")
    );
  }

  #[test]
  fn rejects_subpaths_and_paths_outside_packages() {
    let temp = tempfile::tempdir().unwrap();
    resolve_package_library("plugin/subpath", temp.path()).unwrap_err();

    let plugin = temp.path().join("node_modules/plugin");
    write(
      &plugin.join("package.json"),
      &format!(r#"{{"rolldown":{{"abi":{ABI_VERSION},"devBinary":"../outside/plugin"}}}}"#),
    );
    assert!(
      resolve_package_library("plugin", temp.path())
        .unwrap_err()
        .to_string()
        .contains("rolldown.devBinary")
    );
  }
}
