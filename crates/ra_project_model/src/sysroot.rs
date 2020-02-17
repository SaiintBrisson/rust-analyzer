//! FIXME: write short doc here

use anyhow::{anyhow, bail, Context, Result};
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use ra_arena::{impl_arena_id, Arena, RawId};

#[derive(Default, Debug, Clone)]
pub struct Sysroot {
    crates: Arena<SysrootCrate, SysrootCrateData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SysrootCrate(RawId);
impl_arena_id!(SysrootCrate);

#[derive(Debug, Clone)]
struct SysrootCrateData {
    name: String,
    root: PathBuf,
    deps: Vec<SysrootCrate>,
}

impl Sysroot {
    pub fn core(&self) -> Option<SysrootCrate> {
        self.by_name("core")
    }

    pub fn alloc(&self) -> Option<SysrootCrate> {
        self.by_name("alloc")
    }

    pub fn std(&self) -> Option<SysrootCrate> {
        self.by_name("std")
    }

    pub fn proc_macro(&self) -> Option<SysrootCrate> {
        self.by_name("proc_macro")
    }

    pub fn crates<'a>(&'a self) -> impl Iterator<Item = SysrootCrate> + ExactSizeIterator + 'a {
        self.crates.iter().map(|(id, _data)| id)
    }

    pub fn discover(cargo_toml: &Path) -> Result<Sysroot> {
        let src = get_or_install_rust_src(cargo_toml)?;

        if !src.exists() {
            Err(anyhow!(
                "can't load standard library from sysroot\n\
                {}\n\
                (discovered via `rustc --print sysroot`)\n\
                try running `rustup component add rust-src` or set `RUST_SRC_PATH`",
                src.display(),
            ))?;
        }

        let mut sysroot = Sysroot { crates: Arena::default() };
        for name in SYSROOT_CRATES.trim().lines() {
            let root = src.join(format!("lib{}", name)).join("lib.rs");
            if root.exists() {
                sysroot.crates.alloc(SysrootCrateData {
                    name: name.into(),
                    root,
                    deps: Vec::new(),
                });
            }
        }
        if let Some(std) = sysroot.std() {
            for dep in STD_DEPS.trim().lines() {
                if let Some(dep) = sysroot.by_name(dep) {
                    sysroot.crates[std].deps.push(dep)
                }
            }
        }
        if let Some(alloc) = sysroot.by_name("alloc") {
            if let Some(core) = sysroot.core() {
                sysroot.crates[alloc].deps.push(core);
            }
        }
        Ok(sysroot)
    }

    fn by_name(&self, name: &str) -> Option<SysrootCrate> {
        self.crates.iter().find(|(_id, data)| data.name == name).map(|(id, _data)| id)
    }
}

fn get_or_install_rust_src(cargo_toml: &Path) -> Result<PathBuf> {
    fn try_find_src_path(cargo_toml: &Path) -> Result<PathBuf> {
        if let Ok(path) = env::var("RUST_SRC_PATH") {
            return Ok(path.into());
        }

        let rustc_output = Command::new("rustc")
            .current_dir(cargo_toml.parent().unwrap())
            .args(&["--print", "sysroot"])
            .output()
            .context("rustc --print sysroot failed")?;
        if !rustc_output.status.success() {
            match rustc_output.status.code() {
                Some(code) => bail!(
                    "failed to locate sysroot: rustc --print sysroot exited with code {}",
                    code
                ),
                None => {
                    bail!("failed to locate sysroot: rustc --print sysroot terminated by signal")
                }
            };
        }
        let stdout = String::from_utf8(rustc_output.stdout)?;
        let sysroot_path = Path::new(stdout.trim());
        Ok(sysroot_path.join("lib/rustlib/src/rust/src"))
    }

    fn try_install_rust_src(cargo_toml: &Path) -> Result<PathBuf> {
        let rustup_output = Command::new("rustup")
            .current_dir(cargo_toml.parent().unwrap())
            .args(&["component", "add", "rust-src"])
            .output()
            .context("rustup component add rust-src failed")?;
        if !rustup_output.status.success() {
            match rustup_output.status.code() {
                Some(code) => bail!(
                    "failed to install rust-src: rustup component add rust-src exited with code {}",
                    code
                ),
                None => bail!(
                    "failed to install rust-src: rustup component add rust-src terminated by signal"
                ),
            };
        }
        try_find_src_path(cargo_toml)
    }

    let src = try_find_src_path(cargo_toml)?;
    if !src.exists() {
        try_install_rust_src(cargo_toml)
    } else {
        Ok(src)
    }
}

impl SysrootCrate {
    pub fn name(self, sysroot: &Sysroot) -> &str {
        &sysroot.crates[self].name
    }
    pub fn root(self, sysroot: &Sysroot) -> &Path {
        sysroot.crates[self].root.as_path()
    }
    pub fn root_dir(self, sysroot: &Sysroot) -> &Path {
        self.root(sysroot).parent().unwrap()
    }
    pub fn deps<'a>(self, sysroot: &'a Sysroot) -> impl Iterator<Item = SysrootCrate> + 'a {
        sysroot.crates[self].deps.iter().copied()
    }
}

const SYSROOT_CRATES: &str = "
std
core
alloc
collections
libc
panic_unwind
proc_macro
rustc_unicode
std_unicode
test
alloc_jemalloc
alloc_system
compiler_builtins
getopts
panic_unwind
panic_abort
rand
term
unwind
build_helper
rustc_asan
rustc_lsan
rustc_msan
rustc_tsan
syntax";

const STD_DEPS: &str = "
alloc
alloc_jemalloc
alloc_system
core
panic_abort
rand
compiler_builtins
unwind
rustc_asan
rustc_lsan
rustc_msan
rustc_tsan
build_helper";
