use std::{
    env, fs,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use tempfile::NamedTempFile;

use crate::hooks;

const SKILL_DIR_NAME: &str = "grok-build";

// Embedded skill files
const SKILL_MD: &str = include_str!("../SKILL.md");
const README_MD: &str = include_str!("../README.md");
const README_CN_MD: &str = include_str!("../README-CN.md");
const AGENTS_OPENAI_YAML: &str = include_str!("../agents/openai.yaml");

#[cfg(windows)]
const HOOKS_TEMPLATE: &str = include_str!("../hooks/windows/grok-bridge.json");
#[cfg(not(windows))]
const HOOKS_TEMPLATE: &str = include_str!("../hooks/unix/grok-bridge.json");

#[derive(Clone, Debug)]
pub struct Paths {
    pub skill_root: PathBuf,
    pub installed_binary: PathBuf,
    pub hooks_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let agents_dir = env::var_os("AGENTS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".agents")))
            .ok_or_else(|| anyhow!("无法确定 Agent Skills 目录；请设置 AGENTS_DIR 或 HOME"))?;

        let skill_root = agents_dir.join("skills").join(SKILL_DIR_NAME);
        let platform = current_platform_dir();
        let installed_binary = skill_root.join("bin").join(platform).join(binary_name());

        let hooks_file = hook_file_path()?;

        Ok(Self {
            skill_root,
            installed_binary,
            hooks_file,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct InstallationStatus {
    pub binary_installed: bool,
    pub binary_current: bool,
    pub hooks_configured: bool,
}

impl InstallationStatus {
    pub fn display(&self) -> &'static str {
        if self.hooks_configured && self.binary_current {
            "已安装且为最新版本"
        } else if self.hooks_configured && self.binary_installed {
            "已安装但需要更新"
        } else if self.binary_installed {
            "二进制已安装，但 Hooks 未配置"
        } else {
            "未安装"
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApplyResult {
    pub binary: PathBuf,
    pub hooks: PathBuf,
}

impl ApplyResult {
    pub fn display(&self) -> String {
        format!(
            "二进制 {}，Hooks {}",
            self.binary.display(),
            self.hooks.display()
        )
    }
}

pub fn apply() -> Result<ApplyResult> {
    let source = env::current_exe().context("无法定位当前 EXE")?;
    let paths = Paths::discover()?;
    apply_from(&source, &paths)
}

pub fn apply_from(source: &Path, paths: &Paths) -> Result<ApplyResult> {
    fs::create_dir_all(paths.skill_root.parent().unwrap()).with_context(|| {
        format!(
            "创建 skills 目录失败：{}",
            paths.skill_root.parent().unwrap().display()
        )
    })?;

    copy_executable(source, &paths.installed_binary)?;
    extract_skill_files(&paths.skill_root)?;

    // Install hooks pointing to the stable skill-dir binary
    let hook_status = hooks::install_at_path(&paths.installed_binary, &paths.hooks_file)?;
    if !hook_status.installed {
        bail!("Hooks 安装失败");
    }

    Ok(ApplyResult {
        binary: paths.installed_binary.clone(),
        hooks: paths.hooks_file.clone(),
    })
}

pub fn status(paths: &Paths) -> Result<InstallationStatus> {
    let binary_installed = paths.installed_binary.is_file();
    let binary_current = env::current_exe()
        .ok()
        .filter(|source| binary_installed && source.is_file())
        .map(|source| same_file_contents(&source, &paths.installed_binary))
        .transpose()?
        .unwrap_or(false);

    let hooks_status = hooks::status_at_path(&paths.installed_binary, &paths.hooks_file)?;

    Ok(InstallationStatus {
        binary_installed,
        binary_current,
        hooks_configured: hooks_status.installed,
    })
}

pub fn uninstall() -> Result<bool> {
    let paths = Paths::discover()?;
    let hooks_status = hooks::uninstall_at_path(&paths.installed_binary, &paths.hooks_file)?;
    Ok(hooks_status.changed)
}

fn copy_executable(source: &Path, destination: &Path) -> Result<()> {
    if source.canonicalize().ok().as_ref() == destination.canonicalize().ok().as_ref()
        && destination.exists()
    {
        return Ok(());
    }

    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("安装路径没有父目录：{}", destination.display()))?;
    fs::create_dir_all(parent)?;

    let mut source_file = fs::File::open(source)
        .with_context(|| format!("打开当前 EXE 失败：{}", source.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("在 {} 创建临时文件失败", parent.display()))?;
    std::io::copy(&mut source_file, temporary.as_file_mut())
        .with_context(|| format!("复制 EXE 到 {} 失败", destination.display()))?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    make_executable(temporary.path())?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("替换 EXE 失败：{}", destination.display()))?;
    Ok(())
}

fn extract_skill_files(skill_root: &Path) -> Result<()> {
    fs::create_dir_all(skill_root)?;
    fs::create_dir_all(skill_root.join("agents"))?;
    fs::create_dir_all(skill_root.join("hooks/unix"))?;
    fs::create_dir_all(skill_root.join("hooks/windows"))?;

    fs::write(skill_root.join("SKILL.md"), SKILL_MD)?;
    fs::write(skill_root.join("README.md"), README_MD)?;
    fs::write(skill_root.join("README-CN.md"), README_CN_MD)?;
    fs::write(skill_root.join("agents/openai.yaml"), AGENTS_OPENAI_YAML)?;

    // Write both Unix and Windows hook templates
    fs::write(
        skill_root.join("hooks/unix/grok-bridge.json"),
        include_str!("../hooks/unix/grok-bridge.json"),
    )?;
    fs::write(
        skill_root.join("hooks/windows/grok-bridge.json"),
        include_str!("../hooks/windows/grok-bridge.json"),
    )?;

    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn same_file_contents(left: &Path, right: &Path) -> Result<bool> {
    if let (Ok(left_path), Ok(right_path)) = (left.canonicalize(), right.canonicalize()) {
        if left_path == right_path {
            return Ok(true);
        }
    }
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }

    let mut left = BufReader::new(fs::File::open(left)?);
    let mut right = BufReader::new(fs::File::open(right)?);
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn hook_file_path() -> Result<PathBuf> {
    let grok_home = env::var_os("GROK_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".grok")))
        .ok_or_else(|| anyhow!("无法确定 Grok 主目录；请设置 GROK_HOME 或 HOME"))?;
    Ok(grok_home.join("hooks").join("grok-bridge.json"))
}

fn current_platform_dir() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "windows-x86_64";
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return "windows-arm64";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux-x86_64";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "linux-arm64";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "macos-x86_64";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "macos-arm64";
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "grok-bridge.exe"
    } else {
        "grok-bridge"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_binary_and_configures_hooks() {
        let directory = tempfile::tempdir().unwrap();
        let skill_root = directory.path().join("skills/grok-build");
        let platform = current_platform_dir();
        let installed_binary = skill_root.join("bin").join(platform).join(binary_name());
        let hooks_file = directory.path().join("hooks/grok-bridge.json");

        let source = directory.path().join("source.exe");
        fs::write(&source, b"native-binary").unwrap();

        let paths = Paths {
            skill_root: skill_root.clone(),
            installed_binary: installed_binary.clone(),
            hooks_file: hooks_file.clone(),
        };

        apply_from(&source, &paths).unwrap();

        assert_eq!(fs::read(&installed_binary).unwrap(), b"native-binary");
        assert!(skill_root.join("SKILL.md").is_file());
        assert!(skill_root.join("README.md").is_file());
        assert!(skill_root.join("README-CN.md").is_file());
        assert!(skill_root.join("agents/openai.yaml").is_file());
        assert!(skill_root.join("hooks/unix/grok-bridge.json").is_file());
        assert!(skill_root.join("hooks/windows/grok-bridge.json").is_file());

        let status_result = status(&paths).unwrap();
        assert!(status_result.binary_installed);
        assert!(status_result.hooks_configured);
    }

    #[test]
    fn is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let skill_root = directory.path().join("skills/grok-build");
        let platform = current_platform_dir();
        let installed_binary = skill_root.join("bin").join(platform).join(binary_name());
        let hooks_file = directory.path().join("hooks/grok-bridge.json");

        let source = directory.path().join("source.exe");
        fs::write(&source, b"native-binary").unwrap();

        let paths = Paths {
            skill_root,
            installed_binary,
            hooks_file,
        };

        apply_from(&source, &paths).unwrap();
        let first_hooks = fs::read(&paths.hooks_file).unwrap();

        apply_from(&source, &paths).unwrap();
        let second_hooks = fs::read(&paths.hooks_file).unwrap();

        assert_eq!(first_hooks, second_hooks);
    }

    #[test]
    fn byte_compare_detects_updates() {
        let directory = tempfile::tempdir().unwrap();
        let skill_root = directory.path().join("skills/grok-build");
        let platform = current_platform_dir();
        let installed_binary = skill_root.join("bin").join(platform).join(binary_name());
        let hooks_file = directory.path().join("hooks/grok-bridge.json");

        let source_v1 = directory.path().join("v1.exe");
        let source_v2 = directory.path().join("v2.exe");
        fs::write(&source_v1, b"version-1").unwrap();
        fs::write(&source_v2, b"version-2-different").unwrap();

        let paths = Paths {
            skill_root,
            installed_binary,
            hooks_file,
        };

        apply_from(&source_v1, &paths).unwrap();
        assert!(!same_file_contents(&source_v2, &paths.installed_binary).unwrap());

        apply_from(&source_v2, &paths).unwrap();
        assert_eq!(
            fs::read(&paths.installed_binary).unwrap(),
            b"version-2-different"
        );
    }
}
